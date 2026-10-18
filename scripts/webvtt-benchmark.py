#!/usr/bin/env python3
"""Measure pure WebVTT format work against the unchanged 10k CPU/RSS budgets."""
from proposal_benchmark import main

if __name__ == "__main__":
    main("webvtt-benchmark")
