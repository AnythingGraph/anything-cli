#!/usr/bin/env node

import { runCli } from "../src/index.js";

runCli(process.argv).catch(function handleCliFatalError(error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
});
