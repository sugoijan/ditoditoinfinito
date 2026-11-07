## The chart editor.

editor-title = 編集

editor-title-song = 編集 · { $title }

editor-hint-move = 移動

editor-hint-snap = スナップ

editor-hint-notes = ノーツ

editor-hint-listen = 再生

editor-hint-record = 記録

editor-hint-play = プレイ

editor-hint-menu = メニュー

editor-hint-lane = レーン

editor-hint-place = 配置

editor-badge-fake = F

editor-badge-lift = L

editor-marker-bpm = { $bpm } BPM

editor-marker-stop = 停止 { $seconds }秒

editor-marker-delay = ディレイ { $seconds }秒

editor-marker-warp = ワープ { $beats }拍

editor-marker-speed-beats = スピード { $ratio }×（{ $duration }拍で）

editor-marker-speed-seconds = スピード { $ratio }×（{ $duration }秒で）

editor-marker-scroll = スクロール { $ratio }×

editor-marker-fake = フェイク { $beats }拍

editor-marker-ticks = ティック { $ticks }

editor-marker-combo = コンボ { $hit }/{ $miss }

editor-marker-label = 「{ $label }」

editor-panel-beat = 拍：{ $beat } · 小節：{ $measure } · { $time }

editor-panel-snap = スナップ { $snap }分

editor-view-edit = 編集ビュー

editor-view-play = プレイビュー · { $speed }

editor-view-play-reverse = プレイビュー · { $speed }、リバース

editor-view-compact = コンパクト

editor-panel-audio-hidden = 音声：非表示

editor-panel-audio-reading = 音声：音楽を読み込み中…

editor-panel-audio = { $mode ->
    [waveform] 音声：波形
    [spectrogram] 音声：スペクトログラム
   *[both] 音声：波形とスペクトログラム
}

editor-panel-bpm = { $bpm } BPM

editor-panel-bpm-own = { $bpm } BPM（譜面独自のタイミング）

editor-panel-metronome-on = メトロノーム オン
editor-panel-metronome-off = メトロノーム オフ

editor-panel-clap-on = クラップ オン
editor-panel-clap-off = クラップ オフ

editor-panel-tapped = タップ { $bpm } BPM（{ $taps }回）

editor-panel-tapped-offset = タップ { $bpm } BPM（{ $taps }回） · オフセット { $offset }秒

editor-panel-tapping = { $taps }回タップ中…

editor-panel-here = 現在位置：

editor-panel-notes = { $count }ノーツ

editor-panel-selection = 選択した拍：{ $from }–{ $to }

editor-panel-unsaved = 未保存の変更あり

editor-panel-saved = 保存済み

editor-panel-menu = メニュー

editor-panel-play = プレイ

editor-status-listening = 再生中

editor-status-cannot-save = この曲はここには保存できません。代わりにコピーを書き出せます。

editor-status-range-start = 範囲の始点を設定しました

editor-status-view-edit = 編集ビュー：拍ごとに等間隔

editor-status-view-play = プレイビュー：プレイと同じ表示

editor-status-field-compact = コンパクト表示

editor-status-field-skin = スキンで表示

editor-status-lane = レーン { $lane }

editor-status-long-placed = ロングノーツを配置しました

editor-status-head-set = 始点を設定しました。終点に移動して決定します。「戻る」で取り消します。

editor-status-long-canceled = ロングノーツを取り消しました

editor-status-range-canceled = 範囲を取り消しました

editor-status-tool = { $tool ->
    [hold] ツール：ホールド
    [roll] ツール：ロール
    [mine] ツール：地雷
    [lift] ツール：リフト
    [fake] ツール：フェイク
   *[tap] ツール：タップ / ホールド
}

editor-status-snap = スナップ { $snap }分

editor-status-chart-added = 譜面を追加しました

editor-status-copied = コピーしました

editor-status-saved = 保存しました

editor-status-not-saved = 保存できませんでした：{ $error }

editor-status-metronome-on = メトロノーム オン
editor-status-metronome-off = メトロノーム オフ
editor-status-clap-on = アシストクラップ オン
editor-status-clap-off = アシストクラップ オフ

editor-status-tap-first = 先にYを拍に合わせてタップしてください

editor-status-tempo-not-kept = このテンポはファイルに保存できません

editor-status-tapped = { $beat }拍目に{ $bpm } BPMを設定しました

editor-status-tapped-offset = { $beat }拍目に{ $bpm } BPM、オフセット{ $offset }秒を設定しました

editor-status-no-music = 音楽がないため表示できません

editor-status-audio-hidden = 音声ビュー：非表示

editor-status-audio = { $mode ->
    [waveform] 音声ビュー：波形
    [spectrogram] 音声ビュー：スペクトログラム
   *[both] 音声ビュー：波形とスペクトログラム
}

editor-status-recording = { $beat }拍目から記録中：RかEscapeで停止

editor-status-recording-add = { $beat }拍目から追加記録中：RかEscapeで停止

editor-recorded = { $count ->
    [0] 何も記録されませんでした
   *[other] { $count }ノーツを記録しました
}

editor-recorded-late = 平均{ $ms } ms遅れ、±{ $spread } ms

editor-recorded-early = 平均{ $ms } ms早め、±{ $spread } ms

editor-recorded-on-grid = 平均のずれなし、±{ $spread } ms

editor-status-recovered = 前回の未保存の変更を復元しました。破棄するには保存せずに終了します。

editor-status-new-song = 新しい曲です。保存すると残ります。

editor-menu-title = エディター

editor-menu-listen = 再生

editor-menu-stop = 再生を停止

editor-menu-play-cursor = カーソルからプレイ

editor-menu-play-start = 最初からプレイ

editor-menu-autoplay-cursor = カーソルからオートプレイ

editor-menu-record-cursor = カーソルから記録（R）

editor-menu-record-selection = 選択範囲を記録（R）

editor-menu-record-add-cursor = カーソルから追加で記録（Shift R）

editor-menu-record-add-selection = 選択範囲に追加で記録（Shift R）

editor-menu-metronome-on = メトロノーム：オン（K）
editor-menu-metronome-off = メトロノーム：オフ（K）

editor-menu-clap-on = アシストクラップ：オン（Shift K）
editor-menu-clap-off = アシストクラップ：オフ（Shift K）

editor-menu-apply-tapped = タップBPMを適用：{ $bpm }（Shift Y）

editor-menu-apply-tapped-offset = タップしたBPM（{ $bpm }）とオフセットを適用

editor-menu-undo = 元に戻す

editor-menu-redo = やり直し

editor-menu-tool = { $tool ->
    [hold] ツール：ホールド
    [roll] ツール：ロール
    [mine] ツール：地雷
    [lift] ツール：リフト
    [fake] ツール：フェイク
   *[tap] ツール：タップ / ホールド
}

editor-menu-timing = ここのタイミング…

editor-menu-song = 曲…

editor-menu-chart = 譜面…

editor-menu-cut = 切り取り

editor-menu-delete-selection = 選択範囲を削除

editor-menu-mirror-selection = 選択範囲をミラー

editor-menu-select-all = すべて選択

editor-menu-paste = 貼り付け

editor-menu-export = { $platform ->
    [web] ダウンロード（{ $format }）
   *[desktop] コピーを保存（{ $format }）
}

editor-menu-view-play = プレイビュー

editor-menu-view-edit = 編集ビュー

editor-menu-audio-hide = 音声ビューを非表示（G）
editor-menu-audio-show = 音声ビューを表示（G）

editor-menu-audio-mode = { $mode ->
    [waveform] 音声：波形（Shift G）
    [spectrogram] 音声：スペクトログラム（Shift G）
   *[both] 音声：波形とスペクトログラム（Shift G）
}

editor-menu-field-skin = スキンで表示

editor-menu-field-compact = コンパクト表示

editor-menu-place = ノーツを配置／削除
editor-menu-lane-previous = 前のレーン
editor-menu-lane-next = 次のレーン
editor-menu-range-start = ここから範囲を開始
editor-menu-range-end = ここで範囲を終了
editor-menu-range-clear = 範囲を解除

editor-menu-nudge-earlier = 1行前へずらす
editor-menu-nudge-later = 1行後へずらす

editor-menu-jump-start = 最初へ移動
editor-menu-jump-end = 最後へ移動
editor-menu-measure-previous = 前の小節
editor-menu-measure-next = 次の小節
editor-menu-zoom-in = ズームイン
editor-menu-zoom-out = ズームアウト

editor-menu-tap-tempo = タップテンポ…

editor-menu-leave = エディターを終了

editor-tool-title = ノーツツール

editor-tool-tap = タップ / ホールド
editor-tool-hold = ホールド
editor-tool-roll = ロール

editor-tool-mine = 地雷

editor-tool-lift = リフト

editor-tool-fake = フェイク

editor-save-over-title = 曲を上書き保存しますか？

editor-save-over-body = { $platform ->
    [web] このブラウザーに保存された曲が置き換えられます（インポート元のファイルは変更されません）。ノーツが変わった譜面は、プレイ履歴では新しい譜面として扱われます。
   *[desktop] 「{ $file }」が置き換えられます。元のファイルは、同じ場所に「{ $name }.old」として残ります（エディターを開くたび、最初の保存時に作成）。ノーツが変わった譜面は、プレイ履歴では新しい譜面として扱われます。
}

editor-leave-title = 保存せずに終了しますか？

editor-leave-body = 最後の保存以降の変更は失われます。

editor-leave-new-song-body = { $platform ->
    [web] 新しい曲は残りません。
   *[desktop] 新しい曲は残りません。そのフォルダーは削除されます。
}

editor-leave = 終了

editor-timing-title = タイミング

editor-timing-note = { $beat }拍目の位置です。

editor-timing-item = { $kind ->
    [bpm] BPM：{ $value }
    [stop] 停止：{ $value }
    [delay] ディレイ：{ $value }
    [warp] ワープ：{ $value }
    [speed] スピード：{ $value }
    [scroll] スクロール：{ $value }
    [fake] フェイク：{ $value }
    [timesig] 拍子：{ $value }
    [tickcount] ティック数：{ $value }
    [combo] コンボ：{ $value }
   *[label] ラベル：{ $value }
}

editor-timing-bpm-before = BPM：{ $bpm }（直前の値を継続）

editor-timing-offset = オフセット：{ $seconds }秒

editor-timing-keep-beats = 変更時、ノーツの拍位置を維持

editor-timing-keep-times = 変更時、ノーツの時刻を維持

editor-timing-own-on = 譜面独自のタイミング：オン
editor-timing-own-off = 譜面独自のタイミング：オフ

editor-segment-title = { $kind ->
    [bpm] { $beat }拍目のBPM
    [stop] { $beat }拍目の停止
    [delay] { $beat }拍目のディレイ
    [warp] { $beat }拍目のワープ
    [speed] { $beat }拍目のスピード
    [scroll] { $beat }拍目のスクロール
    [fake] { $beat }拍目のフェイク
    [timesig] { $beat }拍目の拍子
    [tickcount] { $beat }拍目のティック数
    [combo] { $beat }拍目のコンボ
   *[label] { $beat }拍目のラベル
}

editor-segment-note-bpm = ここからのBPM（1分あたりの拍数）です。空欄にすると、このBPM変更を削除します。

editor-segment-note-stop = 秒数です（停止はその行のノーツの後で、ディレイは前で止まります）。空か0で取り除きます。

editor-segment-note-length = 長さ（拍）です。空にすると取り除きます。

editor-segment-note-speed = スクロールスピードの倍率、変化にかかる長さ、単位の順に入力します。例：「1.5 2 beats」「0.5 0.25 s」。

editor-segment-note-scroll = ここからのノーツのスクロールの速さです。1が通常です。

editor-segment-note-time-signature = 「3/4」のように入力します。

editor-segment-note-tick-count = ホールドの1拍あたりのティック数です。0から48まで。

editor-segment-note-combo = ヒット時とミス時に加算するコンボ数です。「2/1」のように入力します。両方同じ数なら、数値1つで指定できます。

editor-segment-note-label = この位置の名前です。

editor-offset-title = オフセット

editor-offset-note = 秒数です。音楽の開始時刻は、オフセットの符号を反転した値になります（StepManiaと同じ符号規則）。

editor-error-not-kept = その値はファイルに保存できません。

editor-error-not-number = 「{ $text }」は数値ではありません。

editor-error-not-whole = 「{ $text }」は整数ではありません。

editor-error-time-signature = 「3/4」のように入力します。

editor-error-meter = 0以上の整数を入力します。

editor-number-value = 値：{ $value }

editor-number-ratio = 倍率：{ $value }

editor-number-duration = 長さ：{ $value }

editor-number-numerator = 分子：{ $value }

editor-number-denominator = 分母：{ $value }

editor-number-hit = ヒット：{ $value }

editor-number-miss = ミス：{ $value }

editor-number-bpm = BPM：{ $value }

editor-number-upper-bpm = 上限BPM：{ $value }

editor-number-beats = 拍
editor-number-seconds = 秒

editor-number-automatic = 自動

editor-number-random = ランダム

editor-number-single-bpm = 単一BPM

editor-number-bpm-range = BPMの範囲

editor-number-step-coarse = 変更幅：大（×10）
editor-number-step-fine = 変更幅：小

editor-number-type-value = 値を入力…

editor-number-reset = 変更をリセット

editor-number-remove = 削除

editor-number-apply = 適用

editor-ok = OK

editor-previous-page = 前のページ
editor-next-page = 次のページ

editor-song-item = { $field ->
    [subtitle] サブタイトル：{ $value }
    [artist] アーティスト：{ $value }
    [title_translit] タイトル（ローマ字）：{ $value }
    [artist_translit] アーティスト（ローマ字）：{ $value }
    [genre] ジャンル：{ $value }
    [credit] クレジット：{ $value }
    [music] 音楽：{ $value }
    [banner] バナー：{ $value }
    [background] 背景：{ $value }
    [preview_start] プレビュー開始：{ $value }秒
    [preview_length] プレビューの長さ：{ $value }秒
    [display_bpm] 表示BPM：{ $value }
   *[title] タイトル：{ $value }
}

editor-song-field = { $field ->
    [subtitle] サブタイトル
    [artist] アーティスト
    [title_translit] タイトル（ローマ字）
    [artist_translit] アーティスト（ローマ字）
    [genre] ジャンル
    [credit] クレジット
    [music] 音楽ファイル
    [banner] バナーファイル
    [background] 背景ファイル
    [preview_start] プレビュー開始（秒）
    [preview_length] プレビューの長さ（秒）
    [display_bpm] 表示BPM
   *[title] タイトル
}

editor-song-title = 曲

editor-song-file-note = 曲のフォルダーにあるファイルの名前です。

editor-song-display-bpm-note = 数値1つ、範囲「100-200」、ランダムなら「*」、空なら譜面のBPMです。

editor-chart-title = 譜面 { $index } / { $count }

editor-chart-difficulty = 難易度：{ $difficulty }

editor-chart-meter = レベル：{ $meter }

editor-chart-name = 名前：{ $name }

editor-chart-description = 説明：{ $description }

editor-chart-credit = 譜面作者：{ $credit }

editor-chart-field = { $field ->
    [name] 名前
    [description] 説明
    [credit] 譜面作者
   *[meter] レベル
}

editor-chart-previous = 前の譜面
editor-chart-next = 次の譜面

editor-chart-add = 譜面を追加…

editor-chart-remove = この譜面を削除…

editor-chart-add-title = 譜面を追加

editor-chart-add-note = 曲のタイミングで空のEdit譜面を作ります。

editor-chart-lanes = { $count }レーン

editor-chart-remove-title = この譜面を削除しますか？

editor-chart-remove-body = ノーツも一緒に削除されます（エディターを終了するまでは元に戻せます）。

editor-chart-remove-yes = 削除

editor-tap-title = タップテンポ

editor-tap-start = 拍に合わせてタップし、適用します。

editor-tap-count = { $taps }回 — 拍に合わせてタップします

editor-tap-estimate = { $bpm } BPM（{ $taps }回）

editor-tap-tap = タップ

editor-tap-apply = BPMを適用

editor-tap-apply-offset = BPMとオフセットを適用

editor-tap-reset = タップをリセット
