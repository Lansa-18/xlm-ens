const fs = require('fs');

const logFile = process.argv[2] || 'canary-results.ndjson';

if (!fs.existsSync(logFile)) {
    console.error(`Log file not found: ${logFile}`);
    process.exit(1);
}

const logData = fs.readFileSync(logFile, 'utf-8');
const lines = logData.split('
').filter(line => line.trim() !== '');

const failures = [];
for (const line of lines) {
    try {
        const entry = JSON.parse(line);
        if (entry.level === 'error') {
            failures.push(entry);
        }
    } catch (error) {
        console.error(`Failed to parse log line: ${line}`);
    }
}

if (failures.length === 0) {
    console.log('No failures found in canary log.');
    process.exit(0);
}

let severity = 'info';
let summary = 'Canary tests failed.';
const details = failures.map(f => {
    if (f.test === 'register' || f.test === 'resolve') {
        severity = 'critical';
    } else if (f.test === 'auction' || f.test === 'subdomain') {
        if (severity !== 'critical') {
            severity = 'warning';
        }
    }
    return `- **Test:** ${f.test}
- **Contract:** ${f.contract}
- **Network:** ${f.network}
- **Error:** ${f.error}`;
}).join('

');

if (severity === 'critical') {
    summary = 'Critical canary tests failed: registration or resolution.';
} else if (severity === 'warning') {
    summary = 'Warning: non-critical canary tests failed.';
}

const output = process.env.GITHUB_OUTPUT;
fs.appendFileSync(output, `severity=${severity}\n`);
fs.appendFileSync(output, `summary=${summary}\n`);
fs.appendFileSync(output, `details<<EOF\n${details}\nEOF\n`);
