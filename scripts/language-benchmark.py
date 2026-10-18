#!/usr/bin/env python3
"""Full 10k language workload; includes cold dictionaries in the 96MiB RSS gate."""
from proposal_benchmark import main

if __name__ == "__main__":
    main("language-benchmark")
