#!/usr/bin/env node

const fs = require("fs");
const { execSync } = require("child_process");

const tag = process.argv[2];
if (!tag) {
  console.error("usage: node scripts/extract-release-notes.js <tag>");
  process.exit(1);
}

const changelogPath = "CHANGELOG.md";
const notesPaths = ["release_notes.md", "RELEASE_NOTES.md", "docs/release_notes.md"];

function readIfExists(path) {
  return fs.existsSync(path) ? fs.readFileSync(path, "utf8") : null;
}

function normalizeTag(input) {
  return input.startsWith("v") ? input : `v${input}`;
}

function extractFromChangelog(changelog, rawTag) {
  const tagName = normalizeTag(rawTag).replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const version = tagName.slice(2);
  const versionEscaped = version.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const re = new RegExp(
    `^##\\s+\\[?(?:${tagName}|${versionEscaped})\\]?[^\\n]*\\n([\\s\\S]*?)(?=^##\\s+|\\Z)`,
    "m",
  );
  const match = changelog.match(re);
  return match ? match[1].trim() : null;
}

function gitSummary(rawTag) {
  try {
    const prev = execSync(`git describe --tags --abbrev=0 ${rawTag}^`, {
      stdio: ["ignore", "pipe", "ignore"],
      encoding: "utf8",
    }).trim();
    if (!prev) return null;
    const log = execSync(`git log --pretty=format:'- %s (%h)' ${prev}..${rawTag}`, {
      stdio: ["ignore", "pipe", "ignore"],
      encoding: "utf8",
    }).trim();
    return log || null;
  } catch {
    return null;
  }
}

const sections = [];
sections.push(`# trace-share ${tag}`);

const changelog = readIfExists(changelogPath);
if (changelog) {
  const extracted = extractFromChangelog(changelog, tag);
  if (extracted) {
    sections.push("## Changelog");
    sections.push(extracted);
  }
}

for (const path of notesPaths) {
  const notes = readIfExists(path);
  if (notes && notes.trim()) {
    sections.push("## Release Notes");
    sections.push(notes.trim());
    break;
  }
}

const summary = gitSummary(tag);
if (summary) {
  sections.push("## Commits");
  sections.push(summary);
}

if (sections.length <= 1) {
  sections.push("Release artifacts for this tag.");
}

process.stdout.write(`${sections.join("\n\n")}\n`);
