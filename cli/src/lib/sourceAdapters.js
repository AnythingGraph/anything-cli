// Supported data source adapters for `anythinggraph source add`.
export const SOURCE_ADAPTERS = [
  {
    number: 1,
    label: "PostgreSQL",
    adapter: "sql",
    description: "PostgreSQL via sqlx",
  },
  {
    number: 2,
    label: "MySQL / MariaDB",
    adapter: "mysql",
    description: "MySQL-compatible databases",
  },
  {
    number: 3,
    label: "SQL Server",
    adapter: "mssql",
    description: "Microsoft SQL Server / Azure SQL",
  },
  {
    number: 4,
    label: "MongoDB",
    adapter: "mongodb",
    description: "MongoDB collections",
  },
  {
    number: 5,
    label: "Salesforce",
    adapter: "soql",
    description: "Salesforce REST / SOQL",
  },
  {
    number: 6,
    label: "CSV file",
    adapter: "csv",
    description: "Local CSV or flat files",
  },
  {
    number: 7,
    label: "REST / HTTP API",
    adapter: "rest",
    description: "HTTP JSON APIs",
  },
];

// Find adapter metadata by menu number.
export function findAdapterByNumber(menuNumber) {
  return SOURCE_ADAPTERS.find(function matchNumber(entry) {
    return entry.number === menuNumber;
  });
}

// Build AG_* env var prefix from a profile source id.
export function buildEnvPrefix(sourceId) {
  const normalized = sourceId
    .trim()
    .toUpperCase()
    .replace(/[^A-Z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "");
  return `AG_${normalized}`;
}

// Validate profile source id (YAML key).
export function isValidSourceId(sourceId) {
  return /^[a-z][a-z0-9_]*$/.test(sourceId);
}
