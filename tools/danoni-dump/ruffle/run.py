#!/usr/bin/env python3
"""Run the experimental worker under macOS restrictions. No unsandboxed fallback."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import resource
import signal
import subprocess
import sys
import time
from datetime import datetime, timezone
from paths import external_directory

ROOT = Path(__file__).resolve().parent


def limits():
    resource.setrlimit(resource.RLIMIT_CPU,(20,20))
    resource.setrlimit(resource.RLIMIT_FSIZE,(40*1024*1024,40*1024*1024))
    resource.setrlimit(resource.RLIMIT_NOFILE,(128,128))
    resource.setrlimit(resource.RLIMIT_CORE,(0,0))


def run(worker, config, output, *, probe=None, timeout=30, memory_mb=2048):
    if sys.platform != "darwin" or not Path("/usr/bin/sandbox-exec").exists():
        raise RuntimeError("Only the tested macOS sandbox is supported; no unsandboxed fallback")
    worker, config, output = worker.resolve(strict=True), config.resolve(strict=True), external_directory(output)
    if output == config.parent or output.is_relative_to(config.parent):
        raise ValueError("Output must be separate from the read-only input directory")
    output.mkdir(parents=True, exist_ok=False)
    identity = {"workerSha256":hashlib.sha256(worker.read_bytes()).hexdigest(),
                "configSha256":hashlib.sha256(config.read_bytes()).hexdigest(),
                "sandboxSha256":hashlib.sha256((ROOT/"worker.sb").read_bytes()).hexdigest()}
    started_at = datetime.now(timezone.utc).isoformat()
    command = ["/usr/bin/sandbox-exec", "-D",f"WORKER={worker}", "-D",f"INPUT={config.parent}",
               "-D",f"OUTPUT={output}", "-f",str(ROOT/"worker.sb"),str(worker)]
    command += ["--probe", *probe] if probe else [str(config)]
    start, peak, reason = time.monotonic(), 0, None
    with (output/"snapshot.json").open("wb") as stdout, (output/"stderr.txt").open("wb") as stderr:
        process = subprocess.Popen(command,cwd=output,stdout=stdout,stderr=stderr,
                                   env={"PATH":"/usr/bin:/bin","HOME":str(output),"TMPDIR":str(output)},
                                   start_new_session=True,preexec_fn=limits)
        try:
            while process.poll() is None:
                if time.monotonic()-start > timeout:
                    reason = "wall-time-limit"
                    break
                rss = subprocess.run(["/bin/ps","-o","rss=","-p",str(process.pid)],
                                     capture_output=True,text=True,timeout=1)
                if rss.stdout.strip():
                    peak = max(peak,int(rss.stdout.strip()))
                elif process.poll() is None:
                    reason = "memory-monitor-unavailable"
                if peak > memory_mb*1024:
                    reason = "resident-memory-limit"
                if reason:
                    break
                time.sleep(0.1)
        except Exception as error:
            reason = f"monitor-error: {type(error).__name__}"
        finally:
            if process.poll() is None:
                os.killpg(process.pid,signal.SIGKILL)
            code = process.wait()
    report = {"schema":"ddi-ruffle-run/experimental-1", "exitCode":code,"termination":reason,
              "startedAt":started_at,"elapsedSeconds":time.monotonic()-start,"peakRssKiB":peak, **identity,
              "limits":{"wallSeconds":timeout,"residentMiB":memory_mb,"cpuSeconds":20,"fileBytes":40*1024*1024}}
    report["inputsUnchanged"] = (identity["workerSha256"] == hashlib.sha256(worker.read_bytes()).hexdigest()
        and identity["configSha256"] == hashlib.sha256(config.read_bytes()).hexdigest()
        and identity["sandboxSha256"] == hashlib.sha256((ROOT/"worker.sb").read_bytes()).hexdigest())
    if not report["inputsUnchanged"]:
        report.update(exitCode=1,termination="inputs-changed-during-run")
    (output/"run.json").write_text(json.dumps(report,indent=2)+"\n")
    return report


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("worker",type=Path)
    parser.add_argument("config",type=Path)
    parser.add_argument("output",type=Path)
    args = parser.parse_args()
    result = run(args.worker,args.config,args.output)
    print(json.dumps(result,indent=2))
    sys.exit(0 if result["exitCode"] == 0 else 1)
