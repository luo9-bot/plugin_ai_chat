import { readFileSync, writeFileSync } from 'fs'
import { resolve, dirname } from 'path'
import { fileURLToPath } from 'url'

const __dirname = dirname(fileURLToPath(import.meta.url))
const html = readFileSync(resolve(__dirname, 'dist/index.html'), 'utf-8')

// Use r##"..."## raw string to avoid escaping issues with backticks in JS
// If HTML contains "##", we'd need more #s, but that's extremely unlikely
const rs = `pub const HTML: &str = r##"${html}"##;\n`
writeFileSync(resolve(__dirname, '../src/admin/ui.rs'), rs)
console.log(`Generated admin_ui.rs (${(rs.length / 1024).toFixed(1)} KB)`)
