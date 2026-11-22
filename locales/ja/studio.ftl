## The announcer studio.

-studio-button-add-recording = 録音を追加
-studio-button-which-line = どのセリフ？

studio-title = アナウンサースタジオ

studio-heading = アナウンサースタジオ

studio-intro = { $platform ->
    [web] 自分の声の録音を下のセリフにドロップするか、セリフの「{ -studio-button-add-recording }」ボタンで追加します。作ったアナウンサーはゲームで使ったり、書き出して共有したりできます。
   *[desktop] 自分の声の録音は、下の各セリフの「{ -studio-button-add-recording }」ボタンで追加します。作ったアナウンサーはゲームで使ったり、書き出して共有したりできます。
}

studio-icons-help = ▶で録音や例を再生します。目のボタンでは、ゲームの中でそのセリフが言われる画面を見られます。

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

studio-languages-none = 録音は1つの言語です。言語を指定するか、別の言語を追加すると、複数言語のパックになります。

studio-languages-one = 録音は{ $language }です。別の言語を追加すると、複数言語のパックになります。

studio-languages-many = 言語ごとに録音が分かれています。ここで選んだ言語の録音が表示されます。

studio-name-language = 言語を指定…
studio-add-language = 言語を追加…

studio-remove-language = { $language }を削除

studio-mixed = { $state ->
   *[off] 言語のミックス：オフ
    [on] 言語のミックス：オン
}

studio-mixed-description = プレイヤーが「ミックス」も選べるようになります。各セリフの録音をすべての言語から選びます。

studio-language-title-name = 録音の言語は？
studio-language-note-name = これまでの録音の言語です。
studio-language-title-add = 言語を追加
studio-language-note-add = セリフは言語ごとに録音します。追加した言語が表示されます。

studio-recorded-in = 録音のある言語：{ $languages }。

studio-take-tags = タグ：{ $tags }

studio-take-tags-button = タグ…

studio-tags = タグ（{ $count }）

studio-tags-help = タグは録音に付ける自分用の単語です。プレイヤーがオフにできるタグは、オプションのサウンドでアナウンサーの下に項目として表示されます。

studio-tag-used = { $count ->
   *[other] { $count }件の録音に付いています。
}

studio-tag-switchable = { $state ->
   *[off] プレイヤーがオフにできる：いいえ
    [on] プレイヤーがオフにできる：はい
}

studio-tag-label = オプションでの名前
studio-tag-label-hint = 表示される名前

studio-tag-default = { $state ->
   *[off] 最初の状態：オフ
    [on] 最初の状態：オン
}

studio-tag-default-description = オン：プレイヤーがオフにするまで、このタグの録音が使われます。オフ：プレイヤーがオンにするまで使われません。

studio-tag-off-instead = オフにすると「{ $line }」の録音がなくなり、代わりに「{ $instead }」が流れます。
studio-tag-off-silent = オフにすると「{ $line }」の録音がなくなり、その場面では何も流れません。
studio-tag-off-shown = オフにすると「{ $line }」の録音がなくなり、その場面は画面に表示されるだけになります。

studio-tag-off-instead-in = オフにすると{ $language }の「{ $line }」の録音がなくなり、代わりに「{ $instead }」が流れます。
studio-tag-off-silent-in = オフにすると{ $language }の「{ $line }」の録音がなくなり、その場面では何も流れません。
studio-tag-off-shown-in = オフにすると{ $language }の「{ $line }」の録音がなくなり、その場面は画面に表示されるだけになります。

studio-tags-title = { $file }のタグ
studio-tags-note = 選ぶとタグを付けたり外したりできます。自分のタグの後に並ぶのはおすすめの単語です。

studio-tag-on = ● { $tag }

studio-tag-new = 新しいタグ…
studio-tag-new-title = 新しいタグ
studio-tag-new-note = 1語で：英小文字、数字、- と _（24文字まで）。
studio-tag-not-a-word = 英小文字、数字、- と _ だけを使ってください（24文字まで）。

studio-take-also = ほかのセリフでも使用：{ $lines }。

studio-take-also-button = ほかのセリフにも…

studio-take-shared-name = { $file }（共有）

studio-take-shared = 「{ $line }」用の録音を、このセリフでも使います。

studio-take-unshare = このセリフでは使わない

studio-also-title = ほかのセリフにも使う
studio-also-note = { $file }は1つのまま保存され、選んだセリフそれぞれで使われます。

studio-lost-refused = スクリプトを実行できませんでした（{ $error }）。録音はフォルダーから読み込みました。
studio-lost-script = スクリプトはそのままでは保存されません。読み込んだ内容から、スタジオが新しく書き出します。
studio-lost-settings = パック独自の設定（{ $names }）は引き継がれません。録音は既定の設定で読み込みました。
studio-lost-by-language = スクリプトがゲームの言語で録音を選んでいます。英語の場合として読み込みました。
studio-lost-weights = 一部の録音の出やすさは引き継がれません。どの録音も同じ確率で使われます。
studio-lost-every-language = { $count ->
   *[other] すべての言語で使われる録音{ $count }件を、{ $language }に入れました。
}
studio-lost-two-languages = { $count ->
   *[other] 2つの言語で使われている録音{ $count }件は、最初の言語に入れました。
}

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

studio-unsorted-help = { $platform ->
    [web] 「{ -studio-button-which-line }」を押すと、録音を聞きながらセリフを選べます。下のセリフにドロップすることもできます。
   *[desktop] 「{ -studio-button-which-line }」を押すと、録音を聞きながらセリフを選べます。
}

studio-section-select = 選曲画面
studio-section-play = プレイ中
studio-section-results = リザルト画面
studio-section-battle = マルチプレイの対戦
studio-section-battle-note = マルチプレイの対戦でのみ再生されます。ここで聞ける場面はまだありません。パックにない場合、その場面は画面に小さく表示されます。

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
studio-line-battle-match-begin = 対戦開始
studio-line-battle-stage-begin = ステージ開始
studio-line-battle-final-stage = ファイナルステージ
studio-line-battle-life-critical = ライフ危険
studio-line-battle-attack-incoming = 攻撃接近
studio-line-battle-attack-launched = 攻撃発動
studio-line-battle-step-up = ステップアップ
studio-line-battle-step-down = ステップダウン
studio-line-battle-survivors = 残り{ $count }人
studio-line-battle-eliminated = ステージからアウト
studio-line-battle-stage-won = ステージ1位
studio-line-battle-stage-not-won = ステージ1位ならず
studio-line-battle-stage-drawn = ステージ1位タイ
studio-line-battle-song-finished = 1位のまま曲の最後まで
studio-line-battle-match-won = 対戦1位
studio-line-battle-match-not-won = 対戦1位ならず
studio-line-battle-match-drawn = 対戦1位タイ

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
studio-when-battle-match-begin = 対戦の最初のステージが始まるときに、「準備」の代わりに再生します。
studio-when-battle-stage-begin = 2つ目以降のステージが始まるときに、「準備」の代わりに再生します。
studio-when-battle-final-stage = 対戦の最後のステージ（曲数を決めた対戦の最後のステージ、またはオープンな対戦で最後の曲にした曲）が始まるときに、「準備」の代わりに再生します。
studio-when-battle-life-critical = プレイヤーのライフが尽きそうになったとき、1ステージに1回再生します。
studio-when-battle-attack-incoming = プレイヤーへの攻撃が決まったときに、ショックが見える前に再生します。
studio-when-battle-attack-launched = プレイヤーのパワーがたまり、自分の攻撃を発動したときに再生します。
studio-when-battle-step-up = プレイヤーが難しい譜面に上がり、その譜面のノーツが始まるときに再生します。
studio-when-battle-step-down = プレイヤーがやさしい譜面に下がり、その譜面のノーツが始まるときに再生します。
studio-when-battle-survivors = ステージに残っているプレイヤーが{ $count }人になったときに再生します。
studio-when-battle-eliminated = プレイヤーのライフが尽き、ステージからアウトになったときに再生します。
studio-when-battle-stage-won = プレイヤーがひとりでステージ1位に決まったときに、曲の途中でも再生します。
studio-when-battle-stage-not-won = 最後まで残ったものの1位になれなかったステージのリザルトで再生します。
studio-when-battle-stage-drawn = 1位を分け合ったステージのリザルトで再生します。
studio-when-battle-song-finished = すでにステージ1位に決まったプレイヤーが曲の最後まで来たときに再生します。
studio-when-battle-match-won = 対戦の最後のステージのリザルトで、対戦でひとり1位のときに再生します。
studio-when-battle-match-not-won = 対戦の最後のステージのリザルトで、対戦で1位になれなかったときに再生します。
studio-when-battle-match-drawn = 対戦の最後のステージのリザルトで、1位を分け合ったときに再生します。

studio-line-folder = フォルダー：{ $folder }

studio-recordings-heading = 自分の録音
studio-examples-heading = 例

studio-example = 「{ $words }」

studio-missing-instead = まだありません。代わりに「{ $line }」が再生されます。

studio-missing-next-line = まだありません。代わりにリザルトの次のセリフが再生されます。
studio-missing-silent = まだ録音がないため、この場面では何も再生されません。
studio-missing-shown = まだありません。その場面は画面に表示されるだけです。

studio-opening-long = 「準備」と「スタート」を合わせると{ $seconds }秒かかります。最初のノーツがそれより早い曲では「スタート」だけを言います。

studio-add-recording = { -studio-button-add-recording }…

studio-add-text = セリフの文を追加

studio-add-text-recording = このセリフの録音を追加…

studio-words-hint = セリフの内容

studio-remove-combo = このセリフを外す

studio-add-combo = コンボのセリフを追加…
studio-add-combo-help = 独自の数え方をするパック（64、128、256…）向け：コンボがその数になったときにセリフを再生します。

studio-combo-title = コンボのセリフを追加
studio-combo-note = セリフを再生するコンボ数（1以上の整数）。

studio-combo-not-a-count = 1以上の整数を入力してください。

studio-combo-listed = { $count }コンボのセリフはもうあります。

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
studio-picker-line-battle = { $line } · マルチプレイ

studio-picker-note = セリフを選んでいる間、「{ $file }」を再生します。

studio-why-opening = 「{ $line }」が先に流れます。プレイはこのセリフで始まります。
studio-why-comment = 「{ $line }」が先に流れます。しばらく何も言っていないと、アナウンサーがコメントします。
studio-why-combo = 「{ $line }」が先に流れます。途中でそのコンボに達しました。
studio-why-on-the-way = 「{ $line }」が先に流れます。ゲームが途中で言うセリフです。
studio-why-in-its-place = 「{ $asked }」はまだ録音がありません。代わりに「{ $line }」が流れます。

studio-status-adding = ファイルを{ $count }個追加しています…

studio-status-added = 録音を{ $added }件追加しました。{ $passed ->
    [0] {""}
   *[other] 音声ではない{ $passed }個のファイルは無視しました。
}

studio-status-draft-unkept = 下書きを保存できませんでした：{ $error }

studio-status-export-stale = パックの作成中に下書きが変更されました。もう一度書き出してください。

studio-status-added-some = 録音を{ $added }件追加しました。{ $left }件は追加しませんでした。録音は1件{ $take } MBまで、下書きは{ $files }件・{ $megabytes } MBまでです。

studio-status-added-unkept = 録音を{ $added }件追加しました。{ $passed ->
    [0] {""}
   *[other] 音声ではない{ $passed }個のファイルは無視しました。
}{ $platform ->
    [web] ブラウザーに保存できませんでした：{ $error }
   *[desktop] 保存できませんでした：{ $error }
}

studio-status-files-unreadable = ファイルを読み込めませんでした：{ $error }

studio-status-draft-unreadable = 下書きを読み込めませんでした：{ $error }

studio-status-draft-set-aside = 下書きを読み込めませんでした（{ $error }）。{ $file }として残してあります。これは新しい下書きです。

studio-status-no-decoder = このブラウザーでは音声をデコードできません。

studio-status-no-sound = 音を出せません：{ $error }

studio-status-unplayable = 再生できませんでした：{ $error }

studio-status-example-unplayable = 例を再生できませんでした：{ $error }

studio-status-example-unreadable = 例を読み込めませんでした：{ $error }

studio-status-exporting = パックを作っています…

studio-status-exported = 「{ $file }」ができました（音声ファイル{ $count }個）。「{ $file }を保存」でダウンロードできます。

studio-status-not-exported = パックを作れませんでした：{ $error }

studio-status-saved = { $platform ->
    [web] { $file }をダウンロードに保存しました。
   *[desktop] { $file }を保存しました。
}

studio-status-not-saved = パックを保存できませんでした。

studio-status-using = ゲームに追加しています…

studio-status-used = { $name }をゲームに追加しました。オプションの「サウンド」→「アナウンサー」で選べます。

studio-status-not-used = 追加できませんでした：{ $error }

studio-status-unread = 作成したパックを、ゲームが作ったとおりに読み込めませんでした（{ $error }）。これはスタジオの不具合で、操作の誤りではありません。何も保存されていません。報告をお願いします。

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

studio-stage-voice-recording = 自分の録音

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

# Variables:
#   $state (String) - "on" or "off"
studio-stadium = { $state ->
    [on] スタジアム効果：オン
   *[off] スタジアム効果：オフ
}

studio-stadium-description = 録音にスタジアムのマイクの音色、エコーと残響を加え、音量をそろえます。試聴と書き出しに反映されます。
