## Multiplayer (a prototype): its home, the room, Rules and More, and
## song select choosing a stage's song.

## The multiplayer home.

room-home-title = マルチプレイ

room-home-heading = マルチプレイ

room-home-practice = CPU練習

room-home-private = プライベートルーム

room-home-find = マッチング · 近日公開
room-home-browse = ルームを見る · 近日公開
room-home-join = リンクで参加
room-sign-in-title = サインイン
room-sign-in-for-private = プライベートルームは招待制で、Twitchのアカウントでプレイします。作るにはTwitchでサインインしてください。
room-sign-in-for-account = Twitchでサインインすると、プライベートルームを作ったり招待に参加したりできます。CPU練習にアカウントは不要です。
room-join-title = リンクで参加
room-join-heading = リンクで参加

## The room.

room-practice-heading = CPU練習

room-players = プレイヤー

room-players-count = { $players } / { $cap }

room-you = あなた

room-cpu-name = CPU { $number }

room-bot-name = { $name }（ボット）

room-cpu-at-start = 開始時

room-state-ready = 準備OK
room-state-not-ready = 未準備
room-state-playing = プレイ中

room-stage = ステージ { $number }

room-stage-empty = 曲が未選択

room-rules = ルール

room-ready = 準備OK
room-unready = 準備を取り消す
room-stop-match = 対戦を中止

## The status line.

room-status-countdown = ステージ { $number } まであと { $seconds }…

room-status-playing = ステージ { $number } をプレイ中です。

room-status-preparing = ステージ { $number } の曲を準備しています…

## Refusals.

room-refused-no-song = 先にこのステージの曲を選んでください。

room-refused-started = そのステージは開始済みのため、変更できません。

room-refused-phase = 対戦のこの段階ではできません。

room-refused-preparing = 曲をまだ準備しています。

room-load-unreadable = 曲を読み込めませんでした。別の曲を選んでください。

room-load-changed = 選んだ後で譜面が変わりました。もう一度選んでください。

room-load-cpu = CPUはこの譜面をプレイできません。別の譜面を選んでください。

room-load-download = ルームの曲をダウンロードできませんでした。

room-load-plan = この端末で出したステージの攻撃がルームのものと合わないため、このステージは見学になります。

room-rules-stage-count = 1対戦のステージ数は1〜{ $max }です。

room-rules-over-cap = CPUを含めて最大{ $cap }人です。

room-rules-too-few = CPUを含めて最低{ $min }人必要です。

room-rules-invalid = このルールは今は使えません。

room-refused-other = 今はできません（{ $code }）。

room-chart-unsupported = CPUがプレイできるのは4レーンの1人用譜面だけです（{ $style }は不可）。別の譜面を選んでください。

room-chart-unsupported-title = CPU非対応

## Song select choosing a stage's song.

room-pick-use = ステージ { $number } に決定

## The Rules pop-up.

room-rules-title = ルール

room-rules-this-match = この対戦 · 固定

room-rules-next-match = 次の対戦

room-rules-stages = ステージ数
room-rules-cpus = CPUの人数
room-rules-level = レベル
room-rules-points = 得点

room-rules-difficulty = タイミング難易度
room-rules-boos = Boo
room-rules-attacks = 攻撃

room-rules-attacks-online = オフ · オンラインルームではまだ使えません

room-rules-points-value = 順位制 · 試験的

room-rules-level-note = CPUの星はレベルを表します。譜面に対する強さで、腕前の評価ではありません。
room-rules-points-note = 8人以外の人数での得点は試験的なルールによります。
room-rules-ruleset-note = 試合では各自のオプションではなく、全員がDDIのこのルールでプレイします。
room-rules-battle-note = バトルは暫定です。ライフが尽きるとそのステージはアウト。攻撃オンでは、タイミングが良いとパワーゲージがたまり、満タンでほかのプレイヤーのフィールドにショックを8つ送ります。
room-rules-locked-note = 変更は次の対戦から適用され、この対戦のルールは変わりません。
room-rules-clears-note = 適用すると準備OKが解除されます。

room-rules-less = −
room-rules-more = +

room-rules-previous = ‹
room-rules-next = ›

room-rules-apply = 適用

## More.

room-leave = ルームを出る

## The room's host.

room-host-options = ホスト設定

room-host-leaves = ホストが退出したとき

room-host-leaves-pass = 引き継ぐ
room-host-leaves-close = ルームを閉じる

room-host-leaves-pass-about = ホストが退出すると、共同ホスト、いなければ最初に参加したプレイヤーがホストになります。
room-host-leaves-close-about = ホストが退出すると、このルームは全員に対して閉じられます。

room-host-people = ほかのプレイヤー

room-host-nobody = まだほかに誰もいません。

room-make-co-host = 共同ホストにする
room-remove-co-host = 共同ホストから外す
room-make-host = ホストにする

room-make-host-title = { $name }をホストにしますか？

room-make-host-body = { $name }がルームを管理し、あなたは共同ホストになります。もう一度ホストになるには、新しいホストに戻してもらう必要があります。

## Asking before leaving a room.

room-leave-title = ルームを出ますか？

room-leave-stay = 残る

room-leave-confirm = 出る

room-leave-host-pass = あなたはホストです。{ $name }がホストになります。

room-leave-host-pass-match = あなたはホストです。{ $name }がホストになり、対戦はあなた抜きで続きます。

room-leave-host-close = あなたはホストです。ルームは全員に対して閉じられます。

room-leave-host-close-match = あなたはホストです。ルームは全員に対して閉じられ、進行中の対戦は終了します。

room-leave-host-alone = ここにはあなたしかいません。ルームは閉じられます。

room-leave-host-stays = あなたはホストです。ルームはホストがいないまま開いた状態になり、リンクから戻れます。

room-leave-playing = 進行中の対戦に参加しています。対戦はあなた抜きで続き、参加しなかったステージは0点になります。

room-leave-picking = あなたが選曲しています。代わりにホストが選曲し、対戦はあなた抜きで続きます。

room-leave-co-host = あなたは共同ホストです。進行中の対戦はあなた抜きで続きます。

## A stage's prompt, results and the room between stages.

room-prompt-stage = ステージ { $number } / { $stages } · CPU { $cpus }人とタップのみの譜面で対戦 · ゲージが尽きるとアウト

room-prompt-stage-online = ステージ { $number } / { $stages } · ほかのプレイヤー{ $others }人とそれぞれの端末で対戦 · ゲージが尽きるとアウト

room-results-back = ルームに戻る

room-prompt-ruleset = ルール：{ $rules }

room-rules-words = { $ruleset }{ $difficulty ->
    [easy] {" "}· イージータイミング
    [hard] {" "}· ハードタイミング
   *[normal] {""}
}{ $boos ->
    [on] {" "}· Booあり
   *[off] {""}
}{ $attacks ->
    [off] {" "}· 攻撃なし
   *[on] {""}
}

room-mark-survived = ステージ { $number }: { $field }人中 { $place }位 · +{ $points }点

room-mark-out = ステージ { $number }: ゲージ切れ · { $field }人中 { $place }位 · +{ $points }点

room-mark-forfeit = ステージ { $number }: 途中でやめた · { $field }人中 { $place }位 · 得点なし

room-mark-experimental = 得点は試験的

room-rank-points = { $rank }位 · { $points }点

room-points-value = { $points }

room-results-title = 対戦結果

room-results-stopped = 対戦中止 · 終わったステージのみ

room-results-winners = 1位: { $names }

room-results-total = 合計

room-results-stage = S{ $number }

room-results-none = —

## A stage's play.

room-play-out = アウト

room-play-won = 1位確定
room-play-cue-match-begin = 対戦スタート
room-play-cue-stage-begin = ステージ { $number } スタート
room-play-cue-final-stage = ファイナルステージ
room-play-cue-life-critical = ライフ危険
room-play-cue-survivors = 残り{ $count }人
room-play-cue-eliminated = アウト
room-play-cue-stage-drawn = 1位タイ
room-play-cue-attack-incoming = 攻撃接近
room-play-cue-attack-launched = 攻撃発動

room-attack-policy-spread = 均等
room-attack-policy-retaliate = 仕返し
room-attack-policy-leader = トップ
room-play-attack-waiting = 攻撃先を待機中
room-targeting = ターゲット
room-attack-policy-spread-about = 攻撃された回数が最も少ない相手を攻撃します。
room-attack-policy-retaliate-about = 最後に自分を攻撃した相手を攻撃します。
room-attack-policy-leader-about = ライフが最も多い相手を攻撃します。
room-results-drawn-final-attack = 最後の攻撃で1位タイ
room-prompt-speed-raised = 攻撃の予告が間に合うようスピードを上げています

## The footer's key hints.

room-hint-move = 移動
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

## The match flow (M7b).

room-status-pick-first = 曲を選んで対戦を始めましょう。

room-status-host-picks-first = ホストが最初の曲を選んでいます。

room-status-pick-next = ステージ{ $number }の曲を選んでください。

room-status-gathering = 全員の準備ができたらステージ{ $number }開始 · { $seconds }秒

room-status-sitting-out = ステージ{ $number }は見学です。ほかのプレイヤーがプレイ中です。

room-status-pick-new-match = 対戦終了。曲を選んで次の対戦を始めましょう。

room-status-stopped-pick = 対戦を中止しました。曲を選んで次の対戦を始めましょう。

room-status-host-picks-new-match = 対戦終了。次の対戦の最初の曲はホストが選びます。

## One picker at a time.

room-status-picker-first = { $name }が最初の曲を選んでいます。

room-status-picker-next = { $name }がステージ{ $number }の曲を選んでいます。

room-status-picker-new-match = 対戦終了。{ $name }が次の対戦の最初の曲を選んでいます。

room-status-nobody-picks = 今は誰も曲を選んでいません。

room-status-nobody-picks-take = 誰も曲を選んでいません。「選曲する」で選べます。

room-take-pick = 選曲する

room-role-picking = 選曲中

room-refused-not-picker = 今はほかのプレイヤーが曲を選んでいます。

room-cpu-style-balanced = バランス
room-cpu-style-stream = 連打
room-cpu-style-jacks = 縦連
room-cpu-style-chords = 同時押し
room-cpu-style-stamina = 体力
room-cpu-style-technical = テクニック
room-cpu-style-custom = カスタム

room-state-loading = 読み込み中
room-state-loading-percent = 読み込み中 { $percent }%
room-state-load-failed = 読み込み失敗
room-state-sitting-out = 見学
room-state-failed-download = ダウンロード失敗
room-state-failed-unreadable = 曲を読めません
room-state-failed-changed = 譜面が変更されました
room-state-failed-plan = 攻撃が不一致

room-panel-points = 得点

room-strip-more = +{ $count }

room-mark-short = { $field }人中 { $place }位 · +{ $points }点
room-mark-short-forfeit = { $field }人中 { $place }位 · 退出

room-picks-heading = 選曲

room-picks-count = { $stages }曲中 { $picked }曲

room-panel-more-above = ▲ ほか{ $count }件
room-panel-more-below = ▼ ほか{ $count }件

room-refused-too-few = ステージを始めるにはプレイヤーが足りません。
room-refused-host-only = ホストだけが行えます。
room-refused-title = できません

room-rules-ok = OK

room-rules-locked-line = { $stages }ステージ · CPU { $cpus }人 · { $rules }

room-rules-no-cpus = CPU対戦相手なし。

room-rules-remove = ×

room-rules-add-random = ランダムに追加
room-rules-add-custom = 選んで追加…

room-add-cpus-title = CPUを追加

room-add-cpus-count = 人数

room-add-cpus-add = CPUを{ $count }人追加

room-add-cpus-by-name = または名前で1人

room-add-cpus-full = 席がいっぱいです。

room-add-cpus-none-left = すべてのCPUが参加済みです。

room-standings-title = ステージ{ $number }の順位

room-standings-continue = 次へ

room-prompt-sat-out = 準備が間に合わなかったため、このステージは見学です。

room-prompt-canceled = ステージの開始は取り消されました。

room-results-tab-standings = 順位
room-results-tab-graphs = グラフ

room-results-hint-tabs = タブ切り替え

room-table-title = ステージ { $number } / { $stages }

room-table-waiting = 全員の結果を待っています…

room-table-player = プレイヤー
room-table-award = ステージ
room-table-total = 合計
room-table-match = 対戦
room-table-score = スコア
room-table-grade = 評価
room-table-life = ライフ
room-table-combo = コンボ
room-table-judgments = 判定

room-table-award-value = +{ $points }

room-table-playing = プレイ中…

room-table-out = アウト { $time }

room-table-left = 退出

room-table-sent = 発動

room-table-taken = 被攻撃

room-table-shocks = ショック

room-summary-sent = 攻撃した相手: { $list }

room-summary-sent-none = 攻撃なし

room-summary-taken = 攻撃してきた相手: { $list }

room-summary-taken-none = 攻撃されず

room-summary-shocks = 踏んだショック: { $count }

room-final-heading = 対戦結果

## 観戦

room-intent-watch = 観戦のみ

room-intent-play = プレイする

room-refused-all-watching = プレイする人がいません。

room-watch-hint = Tabかタップでプレイヤーを選べます
room-live-lost = ルームがステージの中継を失いました。結果は最後に送られます

room-watch-following = { $name }を観戦中

room-watch-cue-out = { $name } アウト

room-watch-cue-won = { $name } 1位確定

room-watch-cue-attack = { $from }が{ $to }を攻撃

room-play-forfeit = 退出

room-play-skip = { $key }: 結果へ

room-play-skip-tap = 結果へ

room-leave-forfeited = このステージを途中でやめました。得点はありません。

room-watched-winner = ステージ1位

room-watched-winners = ステージ1位

room-watched-note = このステージは観戦しました。
