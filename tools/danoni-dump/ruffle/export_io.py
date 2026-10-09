"""Small shared helpers for derived local artifacts."""
import hashlib
import json


def digest(data):
    return hashlib.sha256(data).hexdigest()


def write_json(path,value):
    path.write_text(json.dumps(value,ensure_ascii=False,indent=2)+'\n')
