import { readFile } from 'node:fs/promises';

const reportPath = process.argv[2] ?? 'quality-results/latest.json';
let report;
try {
  report = JSON.parse(await readFile(reportPath, 'utf8'));
} catch (error) {
  console.error(`No readable QA report at ${reportPath}: ${error.message}`);
  process.exit(2);
}

console.log(`Rudiment QA ${report.profile}: ${report.gate.toUpperCase()} (${(report.durationMs / 1000).toFixed(1)}s)`);
for (const result of report.results) {
  const diagnostic = result.reason ? ` — ${result.reason}` : result.log ? ` — ${result.log}` : '';
  console.log(`${result.status.toUpperCase().padEnd(7)} ${result.label}${diagnostic}`);
}
if (report.gate !== 'passed') process.exitCode = 1;
