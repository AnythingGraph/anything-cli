import fs from "node:fs";

// Read a .env file into a key-value map (does not mutate process.env).
export function readEnvFileMap(envFilePath) {
  const values = new Map();
  if (!envFilePath || !fs.existsSync(envFilePath)) {
    return values;
  }

  const rawText = fs.readFileSync(envFilePath, "utf8");
  for (const line of rawText.split("\n")) {
    const trimmedLine = line.trim();
    if (!trimmedLine || trimmedLine.startsWith("#")) {
      continue;
    }

    const equalsIndex = trimmedLine.indexOf("=");
    if (equalsIndex === -1) {
      continue;
    }

    const key = trimmedLine.slice(0, equalsIndex).trim();
    let value = trimmedLine.slice(equalsIndex + 1).trim();
    if (
      (value.startsWith('"') && value.endsWith('"')) ||
      (value.startsWith("'") && value.endsWith("'"))
    ) {
      value = value.slice(1, -1);
    }

    if (key) {
      values.set(key, value);
    }
  }

  return values;
}

// Set or replace one key in a .env file, preserving comments and other keys.
export function setEnvFileValues(envFilePath, keyValuePairs) {
  const entries = Object.entries(keyValuePairs);
  if (entries.length === 0) {
    return;
  }

  let lines = [];
  if (envFilePath && fs.existsSync(envFilePath)) {
    lines = fs.readFileSync(envFilePath, "utf8").split("\n");
  }

  for (const [key, value] of entries) {
    const assignment = `${key}=${value}`;
    let replaced = false;

    for (let lineIndex = 0; lineIndex < lines.length; lineIndex += 1) {
      const trimmedLine = lines[lineIndex].trim();
      if (trimmedLine.startsWith("#") || !trimmedLine.includes("=")) {
        continue;
      }
      const existingKey = trimmedLine.slice(0, trimmedLine.indexOf("=")).trim();
      if (existingKey === key) {
        lines[lineIndex] = assignment;
        replaced = true;
        break;
      }
    }

    if (!replaced) {
      if (lines.length > 0 && lines[lines.length - 1] !== "") {
        lines.push("");
      }
      lines.push(assignment);
    }
  }

  const outputText = lines.join("\n");
  fs.writeFileSync(envFilePath, outputText.endsWith("\n") ? outputText : `${outputText}\n`, "utf8");
}
