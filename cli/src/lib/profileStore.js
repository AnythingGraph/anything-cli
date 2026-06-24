import fs from "node:fs";
import path from "node:path";

// Default profile path under an anything-cli checkout.
export function getProfileFilePath(sourceRoot) {
  return path.join(sourceRoot, "profiles", "local.yaml");
}

// Return true when a source id already exists in profiles/local.yaml.
export function profileHasSourceId(profileText, sourceId) {
  const pattern = new RegExp(`^  ${sourceId}:`, "m");
  return pattern.test(profileText);
}

// List source ids declared under `sources:` in local.yaml.
export function listProfileSourceIds(profileText) {
  const sourceIds = [];
  const lines = profileText.split("\n");
  let insideSources = false;

  for (const line of lines) {
    if (/^sources:\s*$/.test(line)) {
      insideSources = true;
      continue;
    }
    if (insideSources && /^[^\s]/.test(line)) {
      break;
    }
    if (insideSources) {
      const match = line.match(/^  ([a-z][a-z0-9_]*):\s*$/);
      if (match) {
        sourceIds.push(match[1]);
      }
    }
  }

  return sourceIds;
}

// Build YAML block for one source entry (two-space indent under sources:).
export function formatSourceYamlBlock(sourceId, profileFields) {
  const lines = [`  ${sourceId}:`];
  const fieldEntries = Object.entries(profileFields);
  const adapterField = fieldEntries.find(function matchAdapter(entry) {
    return entry[0] === "adapter";
  });
  const otherFields = fieldEntries.filter(function skipAdapter(entry) {
    return entry[0] !== "adapter";
  });
  const orderedFields = adapterField ? [adapterField, ...otherFields] : fieldEntries;

  for (const [fieldName, fieldValue] of orderedFields) {
    lines.push(`    ${fieldName}: ${fieldValue}`);
  }
  return lines.join("\n");
}

// Append a new source block to profiles/local.yaml.
export function appendSourceToProfile(profileFilePath, sourceId, profileFields) {
  const block = formatSourceYamlBlock(sourceId, profileFields);
  let profileText = "";

  if (fs.existsSync(profileFilePath)) {
    profileText = fs.readFileSync(profileFilePath, "utf8");
  }

  if (profileHasSourceId(profileText, sourceId)) {
    throw new Error(`Profile already contains source '${sourceId}'. Choose another name.`);
  }

  if (!profileText.trim()) {
    fs.mkdirSync(path.dirname(profileFilePath), { recursive: true });
    fs.writeFileSync(profileFilePath, `sources:\n${block}\n`, "utf8");
    return;
  }

  if (!/^sources:\s*$/m.test(profileText)) {
    profileText = `${profileText.trimEnd()}\n\nsources:\n${block}\n`;
    fs.writeFileSync(profileFilePath, profileText, "utf8");
    return;
  }

  const trimmed = profileText.trimEnd();
  fs.writeFileSync(profileFilePath, `${trimmed}\n${block}\n`, "utf8");
}

// List env var keys referenced by one source block in profiles/local.yaml.
export function listEnvKeysForSource(profileText, sourceId) {
  const envKeys = [];
  const lines = profileText.split("\n");
  let insideSource = false;

  for (const line of lines) {
    if (new RegExp(`^  ${sourceId}:\\s*$`).test(line)) {
      insideSource = true;
      continue;
    }

    if (insideSource) {
      if (/^  [a-z][a-z0-9_]*:\s*$/.test(line)) {
        break;
      }

      const envMatch = line.match(/^\s+[A-Za-z0-9_]+:\s*env:([A-Z0-9_]+)\s*$/);
      if (envMatch) {
        envKeys.push(envMatch[1]);
      }
    }
  }

  return envKeys;
}

// Remove one source block from profiles/local.yaml.
export function removeSourceFromProfile(profileFilePath, sourceId) {
  if (!fs.existsSync(profileFilePath)) {
    throw new Error(`Profile file not found: ${profileFilePath}`);
  }

  const profileText = fs.readFileSync(profileFilePath, "utf8");
  if (!profileHasSourceId(profileText, sourceId)) {
    throw new Error(`Profile does not contain source '${sourceId}'.`);
  }

  const lines = profileText.split("\n");
  const keptLines = [];
  let skippingSource = false;

  for (const line of lines) {
    if (new RegExp(`^  ${sourceId}:\\s*$`).test(line)) {
      skippingSource = true;
      continue;
    }

    if (skippingSource) {
      if (/^  [a-z][a-z0-9_]*:\s*$/.test(line)) {
        skippingSource = false;
        keptLines.push(line);
      }
      continue;
    }

    keptLines.push(line);
  }

  let outputText = keptLines.join("\n").replace(/\n{3,}/g, "\n\n").trimEnd();
  if (outputText) {
    outputText = `${outputText}\n`;
  }

  fs.writeFileSync(profileFilePath, outputText, "utf8");
}
