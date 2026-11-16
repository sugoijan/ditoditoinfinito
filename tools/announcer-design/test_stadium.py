"""Guards for the approved FFmpeg comparison reference (not the production processor)."""
import unittest
from pathlib import Path
import tempfile

import numpy as np

from kokoro import write_wav
from stadium import opening
from stadium_ffmpeg import StadiumReference as Stadium, close_opening


class OpeningTests(unittest.TestCase):
    def test_spatial_fade_keeps_the_direct_voice_and_original_duration(self):
        color = np.linspace(-0.4, 0.4, 4800, dtype=np.float32)
        direct_gain = 0.86 * 0.78
        direct = color * direct_gain
        full = np.full(6000, 0.1, dtype=np.float32)
        full[:len(color)] += direct
        out = close_opening(full, color, direct_gain, 2880)
        self.assertEqual(len(out), len(color))
        np.testing.assert_array_equal(out[:-2880], full[:1920])
        np.testing.assert_allclose(out[-1], direct[-1], atol=1e-7)
        np.testing.assert_allclose(
            close_opening(direct, color, direct_gain, 2880), direct, atol=1e-7)

    def test_all_countdown_calls_keep_their_duration(self):
        for folder in ("gameplay ready", "gameplay restart", "gameplay here we go normal",
                       "gameplay here we go extra", "gameplay here we go final"):
            self.assertTrue(opening(folder), folder)
        self.assertFalse(opening("battle match begin"))
        self.assertFalse(opening("gameplay 100 combo"))

    def test_ffmpeg_renders_deterministically_with_bounded_tails(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            effect = Stadium(root)
            rate = 24000
            t = np.arange(rate, dtype=np.float32) / rate
            voice = .2 * np.sin(2 * np.pi * 180 * t) * np.sin(np.pi * t) ** 2
            path = root / "voice.wav"
            write_wav(path, voice)
            first, info = effect.render(path, 1, "gameplay 100 combo")
            second, _ = effect.render(path, 1, "gameplay 100 combo")
            np.testing.assert_array_equal(first, second)
            self.assertEqual(len(first), 76800)
            self.assertEqual(info["tail_seconds"], .6)
            self.assertGreater(float(abs(first[48000:]).max()), 0)
            timed, _ = effect.render(path, 1, "gameplay ready")
            self.assertEqual(len(timed), 48000)
            # Changing the tail policy changes only the closing spatial fade.
            np.testing.assert_allclose(timed[:-2880], first[:45120], atol=1e-6)
            write_wav(path, np.zeros(rate, dtype=np.float32))
            silent, _ = effect.render(path, 1, "gameplay ready")
            self.assertFalse(np.any(silent))


if __name__ == "__main__":
    unittest.main()
