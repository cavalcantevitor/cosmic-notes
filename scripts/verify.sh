#!/usr/bin/env bash
set -euo pipefail

echo "========================================================"
echo "COSMIC Notes — Quality Assurance & Verification Gate"
echo "========================================================"

echo "[1/2] Checking compilation across all targets..."
cargo check --all-targets

echo "[2/2] Running full smoke, regression & boundary test suites..."
cargo test --all-targets -- --nocapture

echo "========================================================"
echo "✅ All verification checks passed with zero errors!"
echo "========================================================"
