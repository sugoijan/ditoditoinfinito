## Devices.

devices-audio = { $link ->
    [wired] { $rate } kHz · 出力遅延 約{ $latency } ms（有線・内蔵？）
    [wireless] { $rate } kHz · 出力遅延 約{ $latency } ms（ワイヤレス？）
    [unknown] { $rate } kHz · 出力遅延 不明
   *[none] { $rate } kHz · 出力遅延 約{ $latency } ms
}

devices-audio-named = { $name } · { $rate } kHz · 出力遅延 約{ $latency } ms

devices-display = { $width }×{ $height }（{ $scale }×）

devices-display-refresh = { $width }×{ $height }（{ $scale }×）、約{ $hz } Hz

devices-display-named = { $name } · { $width }×{ $height } · ×{ $scale }

devices-button = ボタン{ $number }

devices-axis = { $direction ->
    [plus] 軸{ $number } +
   *[minus] 軸{ $number } −
}

devices-hat = { $direction ->
    [up] ハット{ $number }の上
    [right] ハット{ $number }の右
    [down] ハット{ $number }の下
   *[left] ハット{ $number }の左
}

devices-face-south = 下ボタン
devices-face-east = 右ボタン
devices-face-west = 左ボタン
devices-face-north = 上ボタン

devices-list = { $first }、{ $rest }
