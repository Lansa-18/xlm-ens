const fs = require('fs');

const path = 'cli/src/main.rs';
let code = fs.readFileSync(path, 'utf8');

// Add dry-run to CLI main
code = code.replace(
    /output: OutputFormat,/,
    `output: OutputFormat,

    /// Simulate the transaction without submitting it
    #[arg(long, global = true)]
    dry_run: bool,`
);

fs.writeFileSync(path, code);
console.log('main.rs updated');
