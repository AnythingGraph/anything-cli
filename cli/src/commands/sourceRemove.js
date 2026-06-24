import readline from "node:readline";
import fs from "node:fs";
import {
  loadConfig,
  resolveAnythingGraphHome,
  buildSourcePaths,
} from "../lib/paths.js";
import { removeEnvFileKeys } from "../lib/envFile.js";
import {
  getProfileFilePath,
  listEnvKeysForSource,
  removeSourceFromProfile,
} from "../lib/profileStore.js";
import {
  reloadReasoningCatalog,
} from "../lib/reasoningApi.js";
import {
  fetchValidatedSourceRows,
  formatSourceRowLabel,
} from "../lib/sourceRows.js";

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

// Print numbered source menu with connection status.
function printSourceMenu(rows) {
  console.log("");
  console.log("Step 1 — Choose a data source to remove");
  console.log("");
  for (let index = 0; index < rows.length; index += 1) {
    console.log(formatSourceRowLabel(rows[index], index + 1));
  }
  console.log("");
}

// Prompt until the user picks a valid menu number.
async function promptSourceChoice(promptInterface, rows) {
  while (true) {
    printSourceMenu(rows);
    const answer = await askQuestion(
      promptInterface,
      "Enter number (or press Enter to cancel): "
    );

    if (!answer) {
      return null;
    }

    const menuNumber = Number.parseInt(answer, 10);
    if (Number.isInteger(menuNumber) && menuNumber >= 1 && menuNumber <= rows.length) {
      return rows[menuNumber - 1];
    }

    console.log("Invalid choice. Enter a number from the list.");
  }
}

// Run the interactive source remove wizard.
export async function runSourceRemoveCommand(args) {
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

  const { reasoningBaseUrl, authHeaders, rows } = await fetchValidatedSourceRows(config, {
    skipValidate: false,
  });

  if (rows.length === 0) {
    console.log("No sources configured.");
    console.log(`Profile: ${profileFilePath}`);
    console.log("Add one with: anythinggraph source add");
    return;
  }

  const promptInterface = createPromptInterface();

  try {
    console.log("AnythingGraph — remove data source");
    console.log(`Profile file: ${profileFilePath}`);
    console.log(`Secrets file: ${envFilePath}`);

    const selectedRow = await promptSourceChoice(promptInterface, rows);
    if (!selectedRow) {
      console.log("Cancelled.");
      return;
    }

    const sourceId = selectedRow.source_id;
    const profileText = fs.readFileSync(profileFilePath, "utf8");
    const envKeys = listEnvKeysForSource(profileText, sourceId);

    console.log("");
    console.log("Step 2 — Removing source...");

    removeSourceFromProfile(profileFilePath, sourceId);
    const removedEnvKeys = removeEnvFileKeys(envFilePath, envKeys);

    try {
      await reloadReasoningCatalog(reasoningBaseUrl, authHeaders);
    } catch (reloadError) {
      console.warn("");
      console.warn(`Removed profile and .env entries, but catalog reload failed: ${reloadError.message}`);
      console.warn("Restart the stack: anythinggraph stop && anythinggraph start");
    }

    console.log("");
    console.log("Removed:");
    console.log(`  profiles/local.yaml → sources.${sourceId}`);
    for (const envKey of removedEnvKeys) {
      console.log(`  .env → ${envKey}`);
    }
  } finally {
    promptInterface.close();
  }
}
