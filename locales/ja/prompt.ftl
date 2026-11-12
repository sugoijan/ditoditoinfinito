## The start prompt.

prompt-start = 開始

prompt-hint-activate = 決定

prompt-modifiers = オプション

prompt-save-as-default = 標準設定に保存

prompt-saved-as-default = 標準設定に保存済み

## The controls diagram.

prompt-unbound = 未設定

prompt-controller-pad = パッド{ $pad }

## The timing line.

prompt-timing-late = 入力が{ $ms } ms遅め

prompt-timing-early = 入力が{ $ms } ms早め

prompt-timing-not-calibrated = タイミング未調整

## The card's other steps.

prompt-access = この曲はフォルダー「{ $folder }」から再生します。ブラウザーが読み取りの許可を改めて求めています。

prompt-allow-folder = 読み取りを許可

prompt-access-refused = フォルダーの読み取りが許可されませんでした。

prompt-interrupted = 中断されました

prompt-interrupted-why = { $platform ->
    [web] タブが非表示になったか、音声が停止されました。
   *[desktop] ウィンドウが非表示になったか、音声の出力先が変わりました。
}

prompt-retry = リトライ

prompt-loading = 読み込み中

## Why the song cannot play.

prompt-song-failed = 曲を読み込めませんでした：{ $error }

prompt-replay-refused = このリプレイは再生できません。{ $why }

prompt-results-refused = このプレイのリザルトは表示できません。{ $why }

prompt-chart-style = この譜面のスタイル（{ $style }）には対応していません。

prompt-no-chart = この曲には{ $chart }番の譜面がありません。

## Quiet lines while the play readies.

prompt-needs-gesture = このブラウザーでは、音声を鳴らすためにタップ、クリック、またはキー入力が必要です。

prompt-video-loading = 動画デコーダーを読み込み中…

prompt-video-progress = 動画デコーダーを読み込み中… { $percent }%

prompt-video-failed = 背景動画を再生できません：{ $error }

prompt-download = 曲をダウンロード中…

prompt-download-done = 曲をダウンロード中… { $done }

prompt-download-progress = 曲をダウンロード中… { $done } / { $total }

prompt-share-reading = 共有曲を読み込み中… { $done } / { $total }曲

## The play.

prompt-play-quit = { $control }を長押しまたは2回押しで終了

prompt-play-quit-this = このボタンを長押しまたは2回押しで終了

prompt-play-quit-hold = { $control }を長押しで終了

prompt-play-quit-hold-this = このボタンを長押しで終了

prompt-play-quit-button = 終了ボタンを長押し、または2回タップ

prompt-play-restart = { $key }を長押しまたは2回押しでリスタート

# CPU prototype controls on the start card and results footer (wraps).
# $seed (String) is the exact reproducible random seed, without grouping.
# $strength (Number) is the ability multiplier, not a promised score.
prompt-cpu-description = CPU · タップのみ · 途中終了なし · 強さ { $strength }× · シード { $seed }

# $skills (String) is the seven comma-separated CPU skill multipliers.
prompt-cpu-skills = スキル: { $skills }
