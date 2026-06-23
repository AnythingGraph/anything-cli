import readline from "node:readline";
import fs from "node:fs";
import path from "node:path";
import {
  loadConfig,
  resolveAnythingGraphHome,
  buildSourcePaths,
  buildServiceUrls,
} from "../lib/paths.js";
import { checkHealthUrl } from "../lib/health.js";
import {
  SOURCE_ADAPTERS,
  findAdapterByNumber,
  buildEnvPrefix,
  isValidSourceId,
} from "../lib/sourceAdapters.js";
import { setEnvFileValues } from "../lib/envFile.js";
import {
  appendSourceToProfile,
  getProfileFilePath,
  profileHasSourceId,
} from "../lib/profileStore.js";
import {
  buildReasoningAuthHeaders,
  validateSourceConnection,
  reloadReasoningCatalog,
  summarizeValidationResponse,
} from "../lib/reasoningApi.js";

// Parse shared CLI flags that accept --home.
function parseHomeOption(args) {
  const options = { home: null };
  for (let index = 0; index < args.length; index += 1) {
    if (args[index] === "--home" && args[index + 1]) {
      options.home = args[index + 1];
      index += 1;
    }
  }
  return options;
}

// Create a readline interface for interactive prompts.
function createPromptInterface() {
  return readline.createInterface({ input: process.stdin, output: process.stdout });
}

// Ask one question and return the trimmed answer.
function askQuestion(promptInterface, questionText) {
  return new Promise(function resolveAnswer(resolve) {
    promptInterface.question(questionText, function handleAnswer(answerText) {
      resolve(answerText.trim());
    });
  });
}

// Print the adapter menu for step 1.
function printAdapterMenu() {
  console.log("");
  console.log("Step 1 — Choose a data source type");
  console.log("");
  for (const adapterEntry of SOURCE_ADAPTERS) {
    console.log(`  ${adapterEntry.number}) ${adapterEntry.label}  (${adapterEntry.adapter})`);
  }
  console.log("");
}

// Prompt until the user picks a valid adapter number.
async function promptAdapterChoice(promptInterface) {
  while (true) {
    printAdapterMenu();
    const answer = await askQuestion(promptInterface, "Enter number: ");
    const menuNumber = Number.parseInt(answer, 10);
    const adapterEntry = findAdapterByNumber(menuNumber);
    if (adapterEntry) {
      return adapterEntry;
    }
    console.log("Invalid choice. Enter a number from the list.");
  }
}

// Prompt for a unique profile source id.
async function promptSourceId(promptInterface, profileText) {
  while (true) {
    const answer = await askQuestion(
      promptInterface,
      "Step 2 — Profile name (source_id) [warehouse_pg]: "
    );
    const sourceId = answer || "warehouse_pg";

    if (!isValidSourceId(sourceId)) {
      console.log("Use lowercase letters, numbers, and underscores. Must start with a letter.");
      continue;
    }

    if (profileHasSourceId(profileText, sourceId)) {
      console.log(`Source '${sourceId}' already exists. Pick another name.`);
      continue;
    }

    return sourceId;
  }
}

// Collect credentials for step 3 based on adapter type.
async function promptCredentials(promptInterface, adapterEntry, envPrefix) {
  console.log("");
  console.log("Step 3 — Connection details");
  console.log("");

  const envValues = {};
  const profileFields = {};
  const validatePayload = {
    adapter: adapterEntry.adapter,
  };

  if (adapterEntry.adapter === "sql" || adapterEntry.adapter === "mysql" || adapterEntry.adapter === "mssql") {
    const label =
      adapterEntry.adapter === "sql"
        ? "PostgreSQL connection string"
        : adapterEntry.adapter === "mysql"
          ? "MySQL connection string"
          : "SQL Server connection string (JDBC-style)";
    const dsn = await askQuestion(promptInterface, `${label}: `);
    if (!dsn) {
      throw new Error("Connection string is required.");
    }
    const envKey = `${envPrefix}_DSN`;
    envValues[envKey] = dsn;
    profileFields.dsn = `env:${envKey}`;
    validatePayload.dsn = dsn;
    return { envValues, profileFields, validatePayload };
  }

  if (adapterEntry.adapter === "mongodb") {
    const dsn = await askQuestion(promptInterface, "MongoDB connection URI: ");
    if (!dsn) {
      throw new Error("MongoDB URI is required.");
    }
    const database =
      (await askQuestion(promptInterface, "Database name [anythinggraph]: ")) || "anythinggraph";
    const dsnKey = `${envPrefix}_DSN`;
    const databaseKey = `${envPrefix}_DATABASE`;
    envValues[dsnKey] = dsn;
    envValues[databaseKey] = database;
    profileFields.dsn = `env:${dsnKey}`;
    profileFields.database = `env:${databaseKey}`;
    validatePayload.dsn = dsn;
    validatePayload.database = database;
    return { envValues, profileFields, validatePayload };
  }

  if (adapterEntry.adapter === "soql") {
    const instanceUrl = await askQuestion(promptInterface, "Salesforce instance URL: ");
    const accessToken = await askQuestion(promptInterface, "Salesforce access token: ");
    if (!instanceUrl || !accessToken) {
      throw new Error("Instance URL and access token are required.");
    }
    const instanceKey = `${envPrefix}_INSTANCE_URL`;
    const tokenKey = `${envPrefix}_ACCESS_TOKEN`;
    envValues[instanceKey] = instanceUrl;
    envValues[tokenKey] = accessToken;
    profileFields.instance_url = `env:${instanceKey}`;
    profileFields.auth = `env:${tokenKey}`;
    validatePayload.instance_url = instanceUrl;
    validatePayload.auth = accessToken;
    return { envValues, profileFields, validatePayload };
  }

  if (adapterEntry.adapter === "csv") {
    const filePath = await askQuestion(promptInterface, "Path to CSV file: ");
    if (!filePath) {
      throw new Error("CSV file path is required.");
    }
    const pathKey = `${envPrefix}_FILE_PATH`;
    envValues[pathKey] = filePath;
    profileFields.file_path = `env:${pathKey}`;
    validatePayload.file_path = filePath;
    return { envValues, profileFields, validatePayload };
  }

  if (adapterEntry.adapter === "rest") {
    const baseUrl = await askQuestion(promptInterface, "REST API base URL: ");
    if (!baseUrl) {
      throw new Error("Base URL is required.");
    }
    const token = await askQuestion(
      promptInterface,
      "Bearer token (optional, press Enter to skip): "
    );
    const baseUrlKey = `${envPrefix}_BASE_URL`;
    envValues[baseUrlKey] = baseUrl;
    profileFields.base_url = `env:${baseUrlKey}`;
    validatePayload.base_url = baseUrl;
    if (token) {
      const tokenKey = `${envPrefix}_TOKEN`;
      envValues[tokenKey] = token;
      profileFields.auth = `env:${tokenKey}`;
      validatePayload.auth = token;
    }
    return { envValues, profileFields, validatePayload };
  }

  throw new Error(`Unsupported adapter: ${adapterEntry.adapter}`);
}

// Ensure reasoning-service is reachable before validation.
async function ensureReasoningService(config) {
  const urls = buildServiceUrls(config);
  const reasoningHealthUrl = urls.reasoning;
  const isHealthy = await checkHealthUrl(reasoningHealthUrl, 4000);
  if (isHealthy) {
    return reasoningHealthUrl.replace(/\/health$/, "");
  }

  console.error("");
  console.error("Reasoning service is not running.");
  console.error("Start the stack in another terminal: anythinggraph start");
  console.error("Then run: anythinggraph source add");
  process.exit(1);
}

// Run the interactive four-step source add wizard.
export async function runSourceAddCommand(args) {
  const options = parseHomeOption(args);
  const homeDirectory = resolveAnythingGraphHome(options.home);
  const config = loadConfig(homeDirectory);

  if (!config) {
    console.error("AnythingGraph is not onboarded yet. Run: anythinggraph onboard");
    process.exit(1);
  }

  config.home = homeDirectory;
  const sourcePaths = buildSourcePaths(config.sourceRoot);
  const profileFilePath = getProfileFilePath(config.sourceRoot);
  const envFilePath = sourcePaths.envPath;

  if (!fs.existsSync(envFilePath) && fs.existsSync(sourcePaths.envExamplePath)) {
    fs.copyFileSync(sourcePaths.envExamplePath, envFilePath);
    console.log(`Created ${envFilePath} from .env.example`);
  }

  let profileText = "";
  if (fs.existsSync(profileFilePath)) {
    profileText = fs.readFileSync(profileFilePath, "utf8");
  }

  const promptInterface = createPromptInterface();

  try {
    console.log("AnythingGraph — add data source");
    console.log(`Profile file: ${profileFilePath}`);
    console.log(`Secrets file: ${envFilePath}`);

    const adapterEntry = await promptAdapterChoice(promptInterface);
    const sourceId = await promptSourceId(promptInterface, profileText);
    const envPrefix = buildEnvPrefix(sourceId);
    const { envValues, profileFields, validatePayload } = await promptCredentials(
      promptInterface,
      adapterEntry,
      envPrefix
    );

    console.log("");
    console.log("Step 4 — Testing connection...");

    const reasoningBaseUrl = await ensureReasoningService(config);
    const authHeaders = buildReasoningAuthHeaders(envFilePath);

    let validationResponse;
    try {
      validationResponse = await validateSourceConnection(
        reasoningBaseUrl,
        validatePayload,
        authHeaders
      );
    } catch (validationError) {
      console.error("");
      console.error(`Connection failed: ${validationError.message}`);
      console.error("Nothing was saved. Fix the connection details and try again.");
      process.exit(1);
    }

    const summary = summarizeValidationResponse(validationResponse);
    console.log(`✓ ${summary}`);

    setEnvFileValues(envFilePath, envValues);
    appendSourceToProfile(profileFilePath, sourceId, {
      adapter: adapterEntry.adapter,
      ...profileFields,
    });

    try {
      await reloadReasoningCatalog(reasoningBaseUrl, authHeaders);
    } catch (reloadError) {
      console.warn("");
      console.warn(`Saved profile and .env, but catalog reload failed: ${reloadError.message}`);
      console.warn("Restart the stack: anythinggraph stop && anythinggraph start");
    }

    console.log("");
    console.log("Saved:");
    console.log(`  profiles/local.yaml → sources.${sourceId}`);
    for (const envKey of Object.keys(envValues)) {
      console.log(`  .env → ${envKey}`);
    }
    console.log("");
    console.log(`Use source_id '${sourceId}' in your binding YAML.`);
    console.log("Next: anythinggraph mcp print-config  (connect Cursor / Claude)");
  } finally {
    promptInterface.close();
  }
}
