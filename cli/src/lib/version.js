import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

// Read CLI version from package.json so npm version bumps stay in sync with --version.
const packageJsonPath = path.join(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
  "..",
  "package.json"
);

const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, "utf8"));

export const CLI_VERSION = packageJson.version;
