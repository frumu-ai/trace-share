#!/usr/bin/env node

const fs = require("fs");
const { execSync } = require("child_process");

const tag = process.argv[2];
if (!tag) {
  console.error("usage: node scripts/extract-release-notes.js <tag>");
  process.exit(1);
}

const changelogPath = "CHANGELOG.md";

function readIfExists(path) {
  return fs.existsSync(path) ? fs.readFileSync(path, "utf8") : null;
}

function normalizeTag(input) {
  return input.startsWith("v") ? input : `v${input}`;
}

function extractFromChangelog(changelog, rawTag) {
  const version = normalizeTag(rawTag).replace(/^v/, "");
  const tagForms = new Set([
    normalizeTag(rawTag).toLowerCase(),
    version.toLowerCase(),
  ]);

  const lines = changelog.split(/\r?\n/);
  let inSection = false;
  const collected = [];

  for (const line of lines) {
    if (line.startsWith("## ")) {
      const heading = line.toLowerCase();
      const matched = [...tagForms].some((t) => heading.includes(`[${t}]`) || heading.includes(` ${t}`));
      if (matched) {
        inSection = true;
        continue;
      }
      if (inSection) {
        break;
      }
    }
    if (inSection) {
      collected.push(line);
    }
  }

  const body = collected.join("\n").trim();
  return body || null;
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

const summary = gitSummary(tag);
if (summary) {
  sections.push("## Commits");
  sections.push(summary);
}

if (sections.length <= 1) {
  sections.push("Release artifacts for this tag.");
}

process.stdout.write(`${sections.join("\n\n")}\n`);
