results-title = リザルト

results-song-list = 選曲へ

results-retry = リトライ

results-watch-again = もう一度見る

results-watch-replay = リプレイを見る

results-activate = 決定

results-more-offset-down = オフセット −1 ms
results-more-offset-up = オフセット +1 ms
results-more-reset-offset = オフセットをリセット

results-more-use-mean = 平均のずれを使う

results-song-offset = 曲のオフセット

results-offset = オフセット

results-offset-value = { $ms } ms

results-use-mean = { $ms } msにする

results-ticks = ティック 成功／ミス

results-mines-hit = 踏んだ地雷

results-stray-presses = 空打ち

results-max-combo = 最大コンボ

results-fast-slow = FAST / SLOW

results-mean-offset = 平均のずれ

results-flare-skill = FLARE SKILL

results-flare-points = { $points }ポイント

results-ffr-total-score = トータルスコア

results-ffr-raw-goods = Raw goods

results-note-autoplay = オートプレイです。このプレイは自己ベストの対象になりません。

results-note-device-changed = プレイ中にオーディオデバイスかディスプレイが変わりました。

results-note-fail-delayed = { $at }に失敗しました。ライフが尽きた後もプレイは続きました（失敗後も継続）。

results-note-fail-at-end = 終了時に失敗と判定されました。ライフは{ $at }に尽き、終了時も0でした。

results-note-not-failed-at-end = ライフは{ $at }に尽きましたが、回復しました。失敗ではありません（終了時に失敗判定）。

results-note-not-failed-off = ライフは{ $at }に尽きました。失敗ではありません（失敗なし）。

results-note-video-dropped-frames = フレーム落ちしていたため、この曲の動画をオフにしました（オプション）。

results-note-video-decoder-behind = デコーダーが追いつかなかったため、この曲の動画をオフにしました（オプション）。

results-note-video-trouble = 動画「{ $name }」を再生できませんでした：{ $problem }。曲の背景を表示しました。

results-note-videos-trouble = { $count ->
   *[other] { $count }本の動画を再生できませんでした：{ $problem }。曲の背景を表示しました。
}

results-note-videos-trouble-first = { $count ->
   *[other] { $count }本の動画を再生できませんでした。「{ $name }」：{ $problem }。曲の背景を表示しました。
}

results-note-scripts-failed = 曲のスクリプト背景が停止しました（{ $error }）。曲の背景を表示しました。

results-note-scripts-slow = 曲のスクリプト背景が停止しました（処理に時間がかかりすぎました）。曲の背景を表示しました。

results-note-scripts-busy = 曲のスクリプト背景が停止しました（処理が多すぎました）。曲の背景を表示しました。

results-note-scripts-memory = 曲のスクリプト背景が停止しました（メモリを使いすぎました）。曲の背景を表示しました。

results-note-scripts-string = 曲のスクリプト背景が停止しました（文字列が長くなりすぎました）。曲の背景を表示しました。

results-note-sound-dropped = { $count ->
   *[other] プレイ中に音が{ $count }回途切れました。タイミングがずれている可能性があります。
}

results-note-not-saved = このプレイを保存できませんでした：{ $error }。

results-played-at = { $when }のプレイです。

results-replay-differs-inputs = リプレイの判定が元のプレイと異なりました。リプレイ時の判定と、入力記録を一括で判定した結果が一致しません。

results-replay-differs-timing = リプレイの判定またはライフが、元のプレイと一致しません。

results-replay-differs-score = リプレイのスコアまたは判定数が、元のプレイと一致しません。

results-replay-not-compared = このリプレイを元のプレイと比較できませんでした。{ $why }

results-replay-history-unreadable = プレイ履歴を読み込めませんでした：{ $error }。

results-replay-history-silent = プレイ履歴から応答がありませんでした。

results-replay-gone = このプレイはもう保存されていません。

results-replay-record-unreadable = このプレイの記録を読み取れません。

results-replay-record-broken = このプレイの記録を読み取れません（{ $detail }）。

results-replay-newer-record = このプレイは新しいバージョンのゲームで保存されたため、このバージョンではリプレイできません。

results-replay-newer-judge = このプレイは新しいバージョンのゲームで判定されたため、このバージョンでは判定が変わってしまいます。

results-replay-log-unreadable = このプレイの入力記録を読み取れません（{ $detail }）。

results-replay-chart-changed = プレイ後にこの譜面が変更されました（ノーツかタイミングがプレイ時と異なります）。リプレイを正しく再現できません。

results-replay-song-unreadable = 曲を読み込めませんでした：{ $error }。

results-replay-older-judge = このプレイは以前のバージョンで判定されたものです。ここでは現在のバージョンで判定するため、当時の表示と異なることがあります。

results-replay-missing-skin = このプレイで使われたノートスキン「{ $skin }」は、現在インストールされていません。リプレイを見るには、もう一度追加する必要があります。

results-replay-missing-song = このプレイの曲はライブラリーにありません（同じノーツの譜面を持つ曲がありません）。

results-panel-timing = タイミング

results-timing-note = 平均 { $mean } ms · σ { $sd }

results-timing-early = −{ $ms } ms 早い

results-timing-late = 遅い +{ $ms } ms

results-panel-life = ライフ

results-life-drop = 最大の減少 { $percent }%（{ $at }）

results-life-no-drop = 減少なし

results-panel-offsets = 曲全体のずれ

results-offsets-range = 上が早い · ±{ $ms } ms

results-offsets-misses = 上が早い · ミス { $misses }

results-panel-lanes = レーン別 · { $judgment }以外

results-panel-lanes-any = レーン別 · 最高判定以外

results-panel-beats = 拍の位置別

results-beats-note = 平均 · σ ms

results-beats-none = 判定されたノーツはありません。

results-beats-not-kept = このプレイは、ノーツの拍内の位置が記録されるようになる前に保存されました。

results-beat = { $n }分

results-beat-other = その他

results-sample-note = サンプルのプレイです。数値は例です。

results-more-title = リザルト
