"""Generate small original SM/WAV import fixtures outside the repository sources."""
from pathlib import Path
import json
import wave
import zipfile

root = Path('target/scratch/cpu-automatic')
root.mkdir(parents=True, exist_ok=True)
cases = []
for name, rows, layout in [
    ('Short', ['1000', '2000', '3000', '0M00', '00L0', '000F', '0010', '0001'], 'dance-single'),
    ('Mixed', ['1000', '0100', '0010', '0001', '1100', '0011', '1000', '1000'] * 16, 'dance-single'),
    ('Long', ['1000', '0100', '0010', '0001'] * 1250, 'dance-single'),
    ('Timing', ['1000', '0100', '1100', '0001'] * 16, 'dance-single'),
    ('NotFour', ['100000', '010000', '001000', '000100'], 'dance-solo'),
]:
    folder = root / name
    folder.mkdir(exist_ok=True)
    groups = [rows[i:i+16] + ['0' * len(rows[0])] * (16-len(rows[i:i+16])) for i in range(0, len(rows), 16)]
    chart = (f'#TITLE:CPU {name};\n#ARTIST:DDI synthetic test;\n#MUSIC:silence.wav;\n'
             f'#OFFSET:-1;\n#BPMS:0=240;\n#NOTES:{layout}::Hard:10:0,0,0,0,0:\n'
             + ',\n'.join('\n'.join(g) for g in groups) + ';\n')
    if name == 'Timing':
        chart = chart.replace('#BPMS:0=240;', '#BPMS:0=120,8=240;\n#STOPS:4=0.3;\n#DELAYS:10=0.2;\n#WARPS:12=2;\n#FAKES:6=1;')
    path = folder / 'chart.sm'
    path.write_text(chart)
    duration = 320 if name == 'Long' else 12
    with wave.open(str(folder / 'silence.wav'), 'wb') as audio:
        audio.setnchannels(1)
        audio.setsampwidth(2)
        audio.setframerate(8000)
        audio.writeframes(b'\0\0' * 8000 * duration)
    cases.append(dict(title='CPU ' + name, path=str(path.resolve()), chart=0, duration=duration))
with zipfile.ZipFile(root / 'pack.zip', 'w', compression=zipfile.ZIP_DEFLATED) as archive:
    for case in cases:
        path = Path(case['path'])
        archive.write(path, 'CPU test/' + path.parent.name + '/chart.sm')
        archive.write(path.with_name('silence.wav'), 'CPU test/' + path.parent.name + '/silence.wav')
(root / 'cases.json').write_text(json.dumps(cases, indent=2) + '\n')
