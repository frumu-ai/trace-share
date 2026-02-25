---
title: trace-share
description: Open data infrastructure for coding-agent training traces.
---

# trace-share

Opt-in tooling to collect, sanitize, and publish coding-agent traces for open model training.

## What This Project Is

`trace-share` is open infrastructure for building high-quality coding-agent datasets from real developer workflows.

The pipeline is designed to:

- collect from approved local tool sources
- sanitize data locally before any upload
- convert traces into consistent Episode records
- publish versioned dataset snapshots for model training

## Why It Exists

Open-source coding models are limited by a lack of high-signal agent interaction data.
Most public data is static code, not iterative workflows with tool use, debugging, and fix loops.

`trace-share` exists to close that gap with reproducible, consent-gated, and privacy-aware data contribution flows.

## Who This Is For

- model trainers building open coding assistants
- companies that want to support shared model infrastructure
- individual contributors who want to donate sanitized traces
- maintainers building parser adapters for additional coding tools

## We Are Looking For Support

This project is seeking support from companies and infrastructure partners for:

- storage and egress credits for dataset artifacts
- index/search hosting capacity
- CI/release compute for larger snapshot builds
- security and privacy review support

If your team wants to help, start with the [Proposal](./proposal/) and [Project Status](./status/).

## We Welcome Contributions

Community contributions are core to this project:

- add parser adapters and source definitions
- improve sanitization and safety coverage
- expand tests and CI reliability
- improve docs and onboarding

Start with [Welcome](./welcome/), then [CLI Usage](./cli/) and [Parser Adapters](./parsers/).

## Start Here

- [Welcome](./welcome/)
- [CLI Usage](./cli/)
- [Parser Adapters](./parsers/)
- [Proposal](./proposal/)
- [Project Status](./status/)
