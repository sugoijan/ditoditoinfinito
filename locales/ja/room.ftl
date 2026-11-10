## Multiplayer (a prototype): its home, the room, Rules and More, and
## song select choosing a stage's song.

## The multiplayer home.

room-home-title = マルチプレイ

room-home-heading = マルチプレイ

room-home-about = 試作版です。この端末でCPU相手に複数曲の対戦を練習できます。サインインも接続も不要です。

room-home-practice = CPU練習

room-home-private = プライベートルーム

room-home-private-note = プライベートルームは招待制で、Twitchでのサインインが必要です。曲の共有とオンラインでのステージのプレイは今後追加されます。

## The room.

room-practice-heading = CPU練習

room-practice-note = 試作版 · この端末のみ

room-players = プレイヤー

room-players-count = { $players } / { $cap }

room-players-line = あなたとCPU { $cpus }人 · レベル { $level } / { $levels }

room-you = あなた

room-cpu-name = CPU { $number }

room-cpu-level = レベル { $level } / { $levels }

room-cpu-at-start = 開始時

room-state-ready = 準備OK
room-state-not-ready = 未準備
room-state-playing = プレイ中

room-points = { $points } pt

room-stage = ステージ { $number }

room-stage-chart = ステージ { $number } · { $chart }

room-stage-empty = 曲が未選択

room-no-song = このステージの曲を選んでください。

room-choose-song = 曲を選ぶ

room-change-song = 曲を変更

room-stage-locked = このステージは開始済みのため、曲は変更できません。

room-rules = ルール

room-ready = 準備OK
room-unready = 準備を取り消す
room-start-match = 対戦開始
room-start-stage = ステージ { $number } 開始
room-cancel-start = 開始を取り消す
room-stop-match = 対戦を中止
room-new-match = 次の対戦

## The status line.

room-status-choose = ステージ { $number } の曲を選んでください。

room-status-not-ready = 準備ができたら「準備OK」を押し、対戦を開始します。

room-status-ready = 準備OKです。いつでも開始できます。

room-status-window = あと { $seconds } 秒で開始します。準備ができていない人はこのステージを休みます。

room-status-countdown = ステージ { $number } まであと { $seconds }…

room-status-playing = ステージ { $number } をプレイ中です。

room-status-preparing = ステージ { $number } の曲を準備しています…

room-status-stopped = 対戦を中止しました。終わったステージの結果は残ります。

room-status-over = 対戦が終わりました。

## Refusals.

room-refused-no-song = 先にこのステージの曲を選んでください。

room-refused-not-ready = 先に「準備OK」を押してください。

room-refused-started = そのステージは開始済みのため、変更できません。

room-refused-phase = 対戦のこの段階ではできません。

room-refused-preparing = 曲をまだ準備しています。

room-load-unreadable = 曲を読み込めませんでした。別の曲を選んでください。

room-load-changed = 選んだ後で譜面が変わりました。もう一度選んでください。

room-load-cpu = CPUはこの譜面をプレイできません。別の譜面を選んでください。

room-load-download = ルームの曲をダウンロードできませんでした。

room-rules-stage-count = 1対戦のステージ数は1〜{ $max }です。

room-rules-over-cap = CPUを含めて最大{ $cap }人です。

room-rules-too-few = CPUを含めて最低{ $min }人必要です。

room-rules-invalid = このルールは今は使えません。

room-refused-other = 今はできません（{ $code }）。

room-chart-unsupported = CPUがプレイできるのは4レーンの1人用譜面だけです（{ $style }は不可）。別の譜面を選んでください。

room-chart-unsupported-title = CPU非対応

## Song select choosing a stage's song.

room-pick-heading = ステージ { $number } の曲

room-pick-use = ステージ { $number } に使う

## The Rules pop-up.

room-rules-title = ルール

room-rules-this-match = この対戦 · 固定

room-rules-next-match = 次の対戦

room-rules-stages = ステージ数
room-rules-cpus = CPUの人数
room-rules-level = CPUのレベル
room-rules-battle = バトル
room-rules-points = 得点

room-rules-level-value = { $level } / { $levels }

room-rules-battle-value = サバイバル · 暫定

room-rules-points-value = 順位制 · 試験的

room-rules-level-note = CPUのレベルは譜面に対する強さで、腕前の評価ではありません。
room-rules-points-note = 8人未満の得点は試験的なルールによります。
room-rules-locked-note = 変更は次の対戦から適用され、この対戦のルールは変わりません。
room-rules-clears-note = 適用すると準備OKが解除されます。

room-rules-less = −
room-rules-more = +

room-rules-apply = 適用

## More.

room-leave = ルームを出る



## A stage's prompt, results and the room between stages.

room-prompt-stage = ステージ { $number } / { $stages } · CPU { $cpus }人とタップのみの譜面で対戦 · ゲージが尽きるとアウト

room-prompt-stage-online = ステージ { $number } / { $stages } · ほかのプレイヤー{ $others }人とそれぞれの端末で対戦 · ゲージが尽きるとアウト

room-results-back = ルームに戻る

room-mark-survived = ステージ { $number }: { $field }人中 { $place }位 · +{ $points }点

room-mark-out = ステージ { $number }: ゲージ切れ · { $field }人中 { $place }位 · +{ $points }点

room-mark-forfeit = ステージ { $number }: 途中でやめた · { $field }人中 { $place }位 · 得点なし

room-mark-experimental = 得点は試験的

room-rank-points = { $rank }位 · { $points }点

room-status-next = 準備ができたら「準備OK」を押し、ステージ { $number } を開始します。

room-results-title = 対戦結果

room-results-stopped = 対戦中止 · 終わったステージのみ

room-results-winners = 1位: { $names }

room-results-total = 合計

room-results-stage = S{ $number }

room-results-none = —

## A stage's play.

room-play-heading = CPU · タップのみの譜面

room-play-level = Lv { $level }

room-play-out = アウト

room-play-won = 1位確定
room-play-cue-match-begin = 対戦スタート
room-play-cue-stage-begin = ステージ { $number } スタート
room-play-cue-final-stage = ファイナルステージ
room-play-cue-life-critical = ライフ危険
room-play-cue-survivors = 残り{ $count }人
room-play-cue-eliminated = アウト
room-play-cue-stage-drawn = 1位タイ

## The footer's key hints.

room-hint-move = 移動
room-hint-select = 決定
room-hint-back = 戻る

## Online rooms.

room-account-signed-in = { $name } でサインイン中

room-account-signed-out = サインインしていません。CPU練習にアカウントは不要です。

room-account-checking = サインイン状態を確認しています…

room-account-unreachable = サーバーに接続できませんでした。CPU練習は使えます。

room-sign-in = Twitchでサインイン
room-sign-out = サインアウト

room-invite-field = 招待リンク
room-invite-field-hint = 招待リンクを貼り付け

room-invite-go = 招待を開く

room-invite-not-one = 招待リンクではありません。

room-sign-in-waiting = ブラウザでTwitchのサインインを済ませてから、ここに戻ってください。

room-sign-in-again-browser = もう一度ブラウザを開く
room-sign-in-cancel = キャンセル

room-sign-in-completing = サインインしています…

room-sign-in-canceled = サインインをキャンセルしました。

room-sign-in-failed = サインインできませんでした（{ $code }）。

room-sign-in-retry = もう一度

room-creating = ルームを作成しています…

room-create-failed = ルームを作成できませんでした（{ $code }）。

## An invite's page.

room-invite-title = 招待

room-invite-heading = 招待

room-invite-reading = 招待を確認しています…

room-invite-host = ホスト：{ $name }

room-invite-members = ルームに{ $count }人

room-invite-open = いま参加できます。準備完了は自分で押します。
room-invite-next-match = 試合中です。観戦して、次の試合から参加します。
room-invite-closed = ホストが新しい参加を締め切りました。
room-invite-full = このルームは満員です。

room-invite-expired = この招待は期限切れです。ホストに新しいリンクをもらってください。
room-invite-rotated = この招待は差し替えられました。ホストに新しいリンクをもらってください。
room-invite-unknown = この招待は存在しません。
room-invite-room-closed = このルームは閉じられました。
room-invite-unreachable = サーバーに接続できませんでした。

room-invite-refused = 参加できませんでした（{ $code }）。

room-invite-join = ルームに参加
room-invite-sign-in = Twitchでサインインして参加

room-invite-joining = 参加しています…

## The online room.

room-link-joining = ルームに接続しています…

room-link-reconnecting = 接続が切れました。再接続しています…

room-link-sign-in-again = サインインの期限が切れました。ルームに戻るにはもう一度サインインしてください。

room-link-closed = このルームは閉じられました。

room-link-left = このルームから退出しました。

room-role-host = ホスト
room-role-co-host = 共同ホスト
room-role-admin = 管理者
room-watching = 観戦
room-next-match = 次の試合から

room-conn-joining = 接続中
room-conn-reconnecting = 再接続中
room-conn-away = 離席

room-online-note = オンライン・試作版

room-host-song = ホストが選んだ曲

room-status-sharing = ステージ{ $number }の曲をルームに共有しています…

room-status-uploading = ステージ{ $number }の曲をルームに共有しています… { $percent }%

room-status-share-failed = ステージ{ $number }の曲を共有できませんでした：{ $reason } 「その他」から再試行してください。

room-status-share-mismatch = ステージ{ $number }に共有された曲には、選んだ譜面がありません。

room-status-share-unavailable = ステージ{ $number }の曲をルームに共有できませんでした。

room-retry-sharing = 共有を再試行

room-status-online-stage = ステージ{ $number }をプレイ中です。全員が終わると結果が出ます。

room-chart-no-digest = この譜面はまだオンラインで選べません。ライブラリの読み込み後にもう一度試してください。

room-copy-invite = 招待リンクをコピー
room-replace-invite = 招待リンクを差し替え
room-sign-in-again = もう一度サインイン

room-invite-copied = 招待リンクをコピーしました。
room-invite-replaced = 招待を差し替えました。古いリンクは使えません。
room-invite-not-replaced = 招待を差し替えられませんでした。

room-status-host-chooses = ステージ{ $number }の曲はホストが選びます。

room-no-song-yet = まだ曲が選ばれていません。
