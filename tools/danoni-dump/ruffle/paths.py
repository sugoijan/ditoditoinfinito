"""Keep captured third-party resources and generated dumps outside every checkout."""
from pathlib import Path


def external_directory(directory):
    directory = Path(directory).resolve()
    for parent in (directory,*directory.parents):
        if (parent/".git").exists():
            raise ValueError(f"Dumps and build outputs must stay outside Git checkouts: {directory}")
    return directory
