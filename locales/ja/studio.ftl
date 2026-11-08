## The announcer studio.

-studio-button-add-recording = 録音を追加
-studio-button-which-line = どのセリフ？

studio-title = アナウンサースタジオ

studio-heading = アナウンサースタジオ

studio-intro = 自分の声の録音を下のセリフにドロップするか、セリフの「{ -studio-button-add-recording }」ボタンで追加します。作ったアナウンサーはゲームで使ったり、書き出して共有したりできます。

studio-name = 名前
studio-name-hint = アナウンサーの名前

studio-author = 作者
studio-author-hint = 作った人

studio-license = ライセンス
studio-license-hint = ほかの人が使える条件

studio-examples = 例：{ $set }

studio-use-in-game = ゲームで使う

studio-export = 書き出す…

studio-save = { $file }を保存

studio-too-big = パックが保存できる上限（音声ファイル{ $files }個、{ $megabytes } MB）を超えています。末尾の音声ファイルは除外されます。

studio-advanced = 詳細設定

studio-import-zip = ZIPからパックをインポート…
studio-import-folder = パックのフォルダーをインポート…

studio-import-description = ここで作ったパックやStepMania向けのアナウンサーパックをインポートできます。録音は対応するセリフに振り分けられます。

studio-tidy = { $state ->
   *[off] 録音を整える：オフ
    [on] 録音を整える：オン
}

studio-tidy-description = 各録音の前後の無音を切り取り、音量をゲーム付属のアナウンサーにそろえます。

studio-stepmania = { $state ->
   *[off] StepMania向けにも：オフ
    [on] StepMania向けにも：オン
}

studio-stepmania-description = StepManiaが読み込める音声形式（MP3、WAV、Ogg Vorbis）で書き出し、それ以外の形式はWAVに変換します。

studio-details = { $state ->
   *[off] 詳細表示：オフ
    [on] 詳細表示：オン
}

studio-details-description = 各セリフのフォルダーと、各録音を整えた内容を表示します。

studio-unsorted = 未分類（{ $count }）

studio-unsorted-help = 「{ -studio-button-which-line }」を押すと、録音を聞きながらセリフを選べます。下のセリフにドロップすることもできます。

studio-section-select = 選曲画面
studio-section-play = プレイ中
studio-section-results = リザルト画面

studio-line-ready = 準備
studio-line-here-we-go = スタート
studio-line-restart = リスタート

studio-line-comment-hot = 絶好調
studio-line-comment-good = 順調
studio-line-comment-danger = ピンチ

studio-line-comment-oni = ライフ制

studio-line-combo = { $combo }コンボ

studio-line-combo-overflow = さらにコンボ

studio-line-combo-stopped = コンボ切れ
studio-line-full-combo = フルコンボ

studio-line-full-combo-w1 = フルコンボ（最高判定のみ）

studio-line-full-combo-w2 = フルコンボ（上位2判定のみ）

studio-line-full-combo-w3 = フルコンボ（上位3判定のみ）

studio-line-last-life = ラストライフ
studio-line-life-lost = ライフ減少
studio-line-cleared = クリア
studio-line-failed = 失敗
studio-line-quit = 中断
studio-line-first-clear = 初クリア
studio-line-new-record = 新記録

studio-line-grade = 評価{ $grade }

studio-line-grade-failed = 失敗したプレイ（{ $grade }）

studio-line-cheer = 声援

studio-line-select-intro = 選曲

studio-line-select-new = 新しい譜面
studio-line-select-repeat = もう一度
studio-line-select-hard = 難しい譜面
studio-line-select-general = ほかの譜面

studio-when-ready = 曲が始まるときに再生します。
studio-when-here-we-go = 最初のノーツの直前に再生し、その0.5秒前に言い終わります。
studio-when-restart = 曲をすぐにリスタートしたときに再生します（Rを長押し、または2回押し）。
studio-when-comment-hot = ライフが満タンの間、10秒ごとに再生します。
studio-when-comment-good = それ以外のとき、10秒ごとに再生します。
studio-when-comment-danger = ライフが尽きそうな間、10秒ごとに再生します。
studio-when-comment-oni = ライフを個数で数えるゲージ（バッテリー）では、10秒ごとに再生します。

studio-when-combo = { $count }ノーツ連続でつないだときに再生します。

studio-when-combo-past-1000 = { $count }ノーツ連続でつないだとき、「さらにコンボ」の代わりに再生します。

studio-when-combo-overflow = 1000コンボを超えてから、100ノーツつなぐごとに再生します。
studio-when-combo-stopped = 50コンボを超えてから、ミスでコンボが途切れたときに再生します。
studio-when-full-combo = ミスなく最後のノーツを叩いたときに再生します。
studio-when-full-combo-w1 = 全ノーツが最高判定なら、最後のノーツを叩いたときに再生します。
studio-when-full-combo-w2 = 全ノーツが上位2判定なら、最後のノーツを叩いたときに再生します。
studio-when-full-combo-w3 = 全ノーツが上位3判定なら、最後のノーツを叩いたときに再生します。
studio-when-last-life = ライフを個数で数えるゲージで、残り1つになったときに再生します。
studio-when-life-lost = ライフを個数で数えるゲージで、1つ減ったときに再生します。
studio-when-cleared = 最後のノーツが判定されたときに再生します。
studio-when-failed = ライフが尽きたときに再生します。
studio-when-quit = プレイヤーが中断した曲のリザルトで再生します。
studio-when-first-clear = 譜面を初めてクリアしたリザルトで再生します。
studio-when-new-record = 以前プレイした譜面の自己ベストを更新したリザルトで再生します。
studio-when-evaluation-full-combo-w1 = 全ノーツが最高判定のフルコンボのリザルトで再生します。
studio-when-evaluation-full-combo-w2 = 全ノーツが上位2判定のフルコンボのリザルトで再生します。
studio-when-evaluation-full-combo-w3 = 全ノーツが上位3判定のフルコンボのリザルトで再生します。
studio-when-evaluation-full-combo = それ以外のフルコンボのリザルトで再生します。

studio-when-grade-aaaaa = 再生されません。この評価を出すルールはありません。

studio-when-grade-aaaa = 最高評価のリザルトで再生します。
studio-when-grade-aaa = 2番目に高い評価のリザルトで再生します。
studio-when-grade-aa = 3番目に高い評価のリザルトで再生します。
studio-when-grade-a = 4番目に高い評価のリザルトで再生します。
studio-when-grade-b = 5番目に高い評価のリザルトで再生します。
studio-when-grade-c = 6番目に高い評価のリザルトで再生します。
studio-when-grade-d = クリアになる最低評価のリザルトで再生します。
studio-when-grade-failed = 失敗した曲のリザルトで再生します。

studio-when-cheer = 3番目に高い評価以上なら、リザルトのセリフの2.5秒後に再生します。
studio-when-select-intro = 選曲画面が表示されるたびに再生します。プレイ後に戻ったときも再生します。
studio-when-select-new = このルールで一度もプレイしていない譜面を選んだときに再生します。
studio-when-select-repeat = 今回の起動中にすでにプレイした譜面を選ぶと再生します。
studio-when-select-hard = レベル10以上の譜面を選んだときに再生します。
studio-when-select-general = それ以外の譜面を選んだときに再生します。

studio-line-folder = フォルダー：{ $folder }

studio-examples-fold = 例（{ $count }）

studio-example = 「{ $words }」

studio-missing-instead = まだありません。代わりに「{ $line }」が再生されます。

studio-missing-next-line = まだありません。代わりにリザルトの次のセリフが再生されます。
studio-missing-silent = まだ録音がないため、この場面では何も再生されません。

studio-opening-long = 「準備」と「スタート」を合わせると{ $seconds }秒かかります。最初のノーツがそれより早い曲では「スタート」だけを言います。

studio-add-recording = { -studio-button-add-recording }…

studio-hear-recorded = ゲームで聞く

studio-hear-example = ゲームで例を聞く

studio-take = { $file } · { $seconds }秒

studio-take-unplayable = { $file } · 再生できません

studio-take-reading = { $file } · 読み込み中…

studio-take-move = 移動…

studio-take-which-line = { -studio-button-which-line }

studio-take-remove = 削除

studio-note-unreadable = このファイルはここでは再生できません（{ $error }）。

studio-note-clipped = 大きすぎる音で録音されています。ピークで音が割れます。

studio-note-too-long = 録音が{ $seconds }秒あるため、次の場面に重なります（{ $limit }秒まで）。

studio-tidied-as-recorded = 録音のまま
studio-tidied-nothing = 整える：変更なし

studio-tidied = { $change ->
    [cut] 整えました：無音を{ $seconds }秒カット
    [louder] 整えました：{ $db } dB大きく
    [quieter] 整えました：{ $db } dB小さく
    [cut-louder] 整えました：無音を{ $seconds }秒カット、{ $db } dB大きく
   *[cut-quieter] 整えました：無音を{ $seconds }秒カット、{ $db } dB小さく
}

studio-picker-title = どのセリフですか？

studio-picker-unsorted = 未分類

studio-picker-line-select = { $line } · 選曲画面

studio-picker-line-play = { $line } · プレイ中

studio-picker-line-results = { $line } · リザルト画面

studio-picker-note = セリフを選んでいる間、「{ $file }」を再生します。

studio-status-adding = ファイルを{ $count }個追加しています…

studio-status-added = 録音を{ $added }件追加しました。{ $passed ->
    [0] {""}
   *[other] 音声ではない{ $passed }個のファイルは無視しました。
}

studio-status-added-unkept = 録音を{ $added }件追加しました。{ $passed ->
    [0] {""}
   *[other] 音声ではない{ $passed }個のファイルは無視しました。
}ブラウザーに保存できませんでした：{ $error }

studio-status-files-unreadable = ファイルを読み込めませんでした：{ $error }

studio-status-draft-unreadable = 下書きを読み込めませんでした：{ $error }

studio-status-no-decoder = このブラウザーでは音声をデコードできません。

studio-status-no-sound = 音を出せません：{ $error }

studio-status-unplayable = 再生できませんでした：{ $error }

studio-status-example-unplayable = 例を再生できませんでした：{ $error }

studio-status-example-unreadable = 例を読み込めませんでした：{ $error }

studio-status-exporting = パックを作っています…

studio-status-exported = 「{ $file }」ができました（音声ファイル{ $count }個）。「{ $file }を保存」でダウンロードできます。

studio-status-not-exported = パックを作れませんでした：{ $error }

studio-status-saved = { $file }をダウンロードに保存しました。

studio-status-not-saved = パックを保存できませんでした。

studio-status-using = ゲームに追加しています…

studio-status-used = { $name }をゲームに追加しました。オプションの「サウンド」→「アナウンサー」で選べます。

studio-status-not-used = 追加できませんでした：{ $error }

studio-status-no-select = 選曲画面を表示できませんでした。
studio-status-no-results = リザルトを表示できませんでした。
studio-status-no-play = プレイを用意できませんでした。

studio-status-heard-nothing = ゲーム内では何も再生されませんでした。

studio-status-heard-nothing-cheer = ゲーム内では何も再生されませんでした。声援はAA以上の評価のリザルトのセリフに続きますが、それらのセリフはまだ録音されていません。

studio-status-heard = ゲーム内で「{ $line }」を再生しました。

studio-status-heard-two = ゲーム内で「{ $first }」、続いて「{ $then }」を再生しました。

studio-status-heard-example = ゲーム内で{ $pack }の例「{ $line }」を再生しました。

studio-status-heard-example-two = ゲーム内で{ $pack }の例「{ $first }」、続いて「{ $then }」を再生しました。

studio-status-heard-with = ゲーム内で「{ $line }」ほか{ $others }件のセリフを再生しました。

studio-status-heard-example-with = ゲーム内で{ $pack }の例「{ $line }」ほか{ $others }件のセリフを再生しました。

studio-stage-kicker = ゲーム内

studio-stage-line = 「{ $line }」

studio-stage-voice-recording = あなたの録音

studio-stage-voice-example = { $pack }の例

studio-stage-back = スタジオに戻る

studio-stage-said = ［{ $line }］

studio-caption-start = スタートメニュー：シングルプレイを選びます。
studio-caption-select-intro = 選曲画面が表示されます。
studio-caption-select-new = 一度もプレイしていない（ベストのない）譜面を選びます。
studio-caption-select-hard = 以前プレイした、レベル10以上の譜面を選びます。
studio-caption-select-repeat = このセッション中にプレイした譜面を選びます。
studio-caption-select-general = 以前プレイした、レベル10未満の譜面を選びます。
studio-caption-quit = プレイヤーが曲の途中で中断します。
studio-caption-first-clear = プレイのあと：譜面を初めてクリアしました。

studio-caption-new-record = プレイ後：{ $score }。この譜面の自己ベストを更新しました。

studio-caption-full-combo-best = フルコンボ：すべてのノーツが{ $judgment }です。

studio-caption-full-combo = フルコンボ：すべてのノーツが{ $judgment }以上です。

studio-caption-failed = ライフが尽きて、曲に失敗します。

studio-caption-graded = 評価{ $grade }のプレイです。

studio-caption-cheer = 評価{ $grade }のプレイです。リザルトのセリフのあとに声援が続きます。

studio-caption-cheer-record = 評価{ $grade }で自己ベストを更新しました。リザルトのセリフのあとに声援が続きます。

studio-caption-cheer-first-clear = 評価{ $grade }で初めてクリアしました。リザルトのセリフのあとに声援が続きます。

studio-caption-before-failed = プレイ前：この譜面のこれまでのプレイは、すべて失敗しています。

studio-caption-before-best = プレイの前：この譜面のベストは{ $score }です。

studio-caption-play-begins = 最初のノーツの数秒前からプレイが始まります。
studio-caption-first-note = 最初のノーツの直前からプレイが始まります。
studio-caption-restart = プレイヤーが曲をリスタートします（Rを2回）。
studio-caption-full-life = ライフ満タンのままプレイが続きます。
studio-caption-below-full = ライフが満タンより少ないままプレイが続きます。
studio-caption-danger = ライフが危険な状態のままプレイが続きます。
studio-caption-battery = バッテリーゲージでプレイが続きます。
studio-caption-combo = コンボが{ $count }に達します。
studio-caption-combo-past = コンボが1000を超えて{ $count }に達します。
studio-caption-combo-broken = 50を超えるコンボが途切れます。
studio-caption-life-lost = バッテリーゲージのライフがひとつ減ります。
studio-caption-last-life = バッテリーゲージのライフが残りひとつになります。
studio-caption-ends-full-combo-best = フルコンボで曲が終わります：すべてのノーツが{ $judgment }です。
studio-caption-ends-full-combo = フルコンボで曲が終わります：すべてのノーツが{ $judgment }以上です。
studio-caption-cleared = 最後のノーツが判定され、曲をクリアします。
