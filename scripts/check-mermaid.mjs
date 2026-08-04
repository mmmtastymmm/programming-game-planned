// Parse every ```mermaid block in the docs with the real mermaid parser.
//
// Why this exists: a broken diagram is invisible in review and in `git diff` —
// it only shows up as a parse error when someone renders the page. The class is
// easy to hit, because mermaid treats characters that read as ordinary prose as
// syntax (`;` terminates a statement, so "pause & save up; more text" silently
// truncates the message and then fails on the remainder).
//
// Reports absolute file:line by mapping mermaid's block-relative line number
// back through the fence position.

import fs from "fs";
import path from "path";
import { JSDOM } from "jsdom";

const dom = new JSDOM("<!doctype html><body></body>");
global.window = dom.window;
global.document = dom.window.document;

const mermaid = (await import("mermaid")).default;
mermaid.initialize({ startOnLoad: false });

const root = path.resolve(process.argv[2] ?? "docs");

function markdownFiles(dir) {
  return fs.readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) return markdownFiles(full);
    return full.endsWith(".md") ? [full] : [];
  });
}

// mermaid reports "Parse error on line N" relative to the block; block line 1
// is the line after the fence, so absolute = fenceLine + N.
function absoluteLine(message, fenceLine) {
  const m = /Parse error on line (\d+)/.exec(message ?? "");
  return m ? fenceLine + Number(m[1]) : fenceLine;
}

let blocks = 0;
const failures = [];

for (const file of markdownFiles(root).sort()) {
  const lines = fs.readFileSync(file, "utf8").split("\n");
  let buf = null;
  let fenceLine = 0;

  for (let i = 0; i < lines.length; i++) {
    const trimmed = lines[i].trim();
    if (buf === null && trimmed.startsWith("```mermaid")) {
      buf = [];
      fenceLine = i + 1; // 1-based line of the fence itself
      continue;
    }
    if (buf !== null && trimmed.startsWith("```")) {
      blocks++;
      try {
        await mermaid.parse(buf.join("\n"));
      } catch (err) {
        const msg = String(err?.message ?? err);
        failures.push({
          file: path.relative(process.cwd(), file),
          line: absoluteLine(msg, fenceLine),
          message: msg.split("\n").slice(0, 4).join("\n    "),
        });
      }
      buf = null;
      continue;
    }
    if (buf !== null) buf.push(lines[i]);
  }

  if (buf !== null) {
    failures.push({
      file: path.relative(process.cwd(), file),
      line: fenceLine,
      message: "unterminated ```mermaid block (no closing fence)",
    });
  }
}

for (const f of failures) {
  console.error(`\n✗ ${f.file}:${f.line}\n    ${f.message}`);
}

if (failures.length > 0) {
  console.error(
    `\n${failures.length} of ${blocks} mermaid block(s) failed to parse.`,
  );
  process.exit(1);
}

console.log(`✓ ${blocks} mermaid blocks parse clean`);
