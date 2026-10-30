"""Locate reviewed example inputs and operator tools for isolated regressions."""
from pathlib import Path
import sys

REPOSITORY = Path(__file__).resolve().parents[3]
EXAMPLE = REPOSITORY / 'example/kubernetes'
TOOLS = REPOSITORY / 'scripts/ops/kubernetes'
sys.path.insert(0, str(TOOLS))
