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
room-sign-in-for-private-host = プライベートルームは招待制で、Twitchのアカウントでプレイします。作るにはTwitchでサインインしてください。あわせて、チャンネルでバンやタイムアウトされた人をゲームが確認することの許可もTwitchで求められます。その人たちはルームに入れなくなります（ホスト設定 → Twitchのバン）。
room-sign-in-for-account = Twitchでサインインすると、プライベートルームを作ったり招待に参加したりできます。CPU練習にアカウントは不要です。
room-join-title = リンクで参加
room-join-heading = リンクで参加

## The room.

room-practice-heading = CPU練習

room-players = プレイヤー

room-players-count = { $players } / { $cap }

room-you = 自分

room-cpu-name = CPU { $number }

room-bot-name = { $name }（ボット）

room-cpu-at-start = 開始時

room-state-ready = 準備OK
room-state-not-ready = 未準備
room-state-playing = プレイ中

room-stage = ステージ{ $number }

room-stage-empty = 曲が未選択

room-rules = ルール

room-ready = 準備OK
room-unready = 準備を取り消す
room-stop-match = 対戦を中止
room-end-match = 今すぐ対戦を終える
room-end-match-body = { $count ->
   *[other] プレイした{ $count }曲のポイントで、今すぐ対戦を終えます。次の対戦も同じルールです。
}
room-ready-last-song = 最後の曲
room-ready-chart = 自分の譜面
room-make-last = この曲を最後の曲にする
room-not-last = 最後の曲にしない
room-pick-last-next = 次に選ぶ曲を最後の曲にする

## The status line.

room-status-countdown = ステージ{ $number }まであと{ $seconds }…

room-status-countdown-soon = まもなくステージ{ $number }が始まります。

room-status-playing = ステージ{ $number }をプレイ中です。

room-status-enter-late = ステージ{ $number }が始まりました。今すぐ「準備OK」を押すと、途中から参加できます。

room-status-preparing = ステージ{ $number }の曲を準備しています…

## Refusals.

room-refused-no-song = 先にこのステージの曲を選んでください。

room-refused-started = そのステージは開始済みのため、変更できません。

room-back-here = { $plays ->
    [this] ルームに戻りました。ステージ{ $number }をプレイします。
    [next] ルームに戻りました。ステージ{ $number }はすでに始まっています。次のステージからプレイします。
   *[none] ルームに戻りました。
}
room-refused-phase = 対戦のこの段階ではできません。

room-refused-preparing = 曲をまだ準備しています。

room-load-unreadable = 曲を読み込めませんでした。別の曲を選んでください。

room-load-changed = 選んだ後で譜面が変わりました。もう一度選んでください。

room-load-cpu = CPUはこの譜面をプレイできません。別の譜面を選んでください。

room-load-download = ルームの曲をダウンロードできませんでした。
room-retry-load = 再試行
room-retry-download = ダウンロードを再試行

room-load-plan = このデバイスで計算したステージの攻撃がルームと一致しないため、このステージは見学になります。

room-rules-stage-count = 対戦のステージ数は1〜{ $max }です。

room-rules-over-cap = CPUを含めて最大{ $cap }人です。

room-rules-too-few = CPUを含めて最低{ $min }人必要です。

room-rules-invalid = このルールは今は使えません。

room-refused-other = 今はできません（{ $code }）。

room-chart-unsupported = CPUがプレイできるのは4レーンの1人用譜面だけです（{ $style }は不可）。別の譜面を選んでください。

room-chart-unsupported-title = CPU非対応

## Song select choosing a stage's song.

room-pick-use = ステージ{ $number }に決定
room-pick-use-missing = ステージ{ $number }に決定（{ $count }人がまだ持っていません）

## The Rules pop-up.

room-rules-title = ルール

room-rules-this-match = この対戦 · 固定

room-rules-next-match = 次の対戦

room-rules-stages = ステージ数
room-rules-stages-open = オープン
room-rules-stages-open-about = オープン：曲数を決めません。選んだ曲を最後の曲にするか、曲のあいだに対戦を終えられます。最大{ $max }曲です。
room-rules-mid-match = 対戦途中の参加
room-rules-mid-match-about = オン：対戦中に入った人は、まだ始まっていない次の曲から参加します。それまでの曲のポイントはありません。
room-rules-cpus = CPUの人数
room-rules-level = レベル
room-rules-points = ポイント

room-rules-difficulty = タイミング難易度
room-rules-boos = Boo
room-rules-attacks = 攻撃

room-rules-at-once = 同時攻撃数
room-rules-at-once-count = { $count }
room-rules-at-once-no-limit = 制限なし
room-rules-at-once-about = 1人のプレイヤーに同時に向かう攻撃の最大数です。攻撃どうしが重なることはありません。

room-rules-shocks = 攻撃ごとのショック数
room-rules-shocks-count = { $count }
room-rules-shocks-about = 1回の攻撃で相手のフィールドに送るショックの数です。

room-rules-attacks-online = オフ · オンラインルームではまだ使えません

room-rules-charts = 譜面
room-rules-life-out = ライフがなくなったら
room-rules-life-out-play-on = プレイ続行
room-rules-life-out-out = アウト
room-rules-life-out-about-play-on = プレイ続行：ライフがなくなった人（適応譜面では最も易しい譜面で）は失敗として曲の最後までプレイし、生き残った全員より下に、失敗した人どうしはスコア順に並びます。
room-rules-life-out-about-out = アウト：ライフがなくなると（適応譜面では最も易しい譜面で）、そのステージから脱落します。
room-rules-charts-as-picked = 選んだまま
room-rules-charts-adaptive = 適応

room-rules-charts-picked-about = 全員が選ばれた譜面をプレイします。
room-rules-charts-adaptive-about = 各プレイヤーが自分の譜面から始め、実力に合わせて上下します。

room-rules-charts-no-ladder = 選ばれた曲にはこのスタイルの譜面がほかにないため、選んだままと同じになります。

room-rules-charts-online = 選んだまま · オンラインルームではまだ使えません

room-rules-points-strong = 強め
room-rules-points-medium = 中くらい
room-rules-points-mild = 弱め

room-rules-points-strong-about = 上位ほど多くのポイントを獲得します。大人数では、下位の獲得ポイントは小さくなります。
room-rules-points-medium-about = 中間です。上位が有利なまま、下位も少し多くポイントを獲得します。
room-rules-points-mild-about = 大人数でも下位までポイントが行き渡り、上位との差は小さくなります。

room-rules-level-note = CPUの星はレベルを表します。ついていける譜面の目安で、腕前の評価ではありません。
room-rules-points-bias-note = ポイントの設定は、8人以下では何も変わりません。
room-rules-ruleset-note = 対戦では各自のオプションに関係なく、全員がこのルールのDDIでプレイします。
room-rules-battle-note = バトルは暫定版です。ライフがなくなると、そのステージはそこで終わりです。攻撃オンでは、タイミングが良いとパワーゲージがたまり、満タンになるとほかのプレイヤーのフィールドに攻撃としてショックをいくつか送ります。
room-rules-locked-note = 変更は次の対戦から適用され、この対戦のルールは変わりません。
room-rules-clears-note = 適用すると準備OKが解除されます。

room-rules-less = −
room-rules-more = +

room-rules-previous = ‹
room-rules-next = ›

room-rules-apply = 適用

room-rules-title-next = 次の対戦のルール

room-rules-running = { $stages ->
    [0] この対戦はステージ{ $number }です。ルールは次のまま変わりません：
   *[other] この対戦はステージ{ $number } / { $stages }です。ルールは次のまま変わりません：
}

room-rules-running-next = { $marked ->
    [yes] 以下は次の対戦のルールです。●はこの対戦と異なる項目です。
   *[no] 以下は次の対戦のルールです。
}

room-rules-apply-restart = 適用してやり直す

room-rules-restart-title = 対戦をやり直しますか？
room-rules-restart-body = この対戦は今すぐ中止され、これまでのポイントは引き継がれません。次に選ばれた曲から、このルールで新しい対戦が始まります。
room-rules-restart-confirm = やり直す

room-rules-title-from = ステージ{ $number }からのルール

room-rules-running-stage = { $stages ->
    [0] この対戦はステージ{ $number }です。現在のルール：
   *[other] この対戦はステージ{ $number } / { $stages }です。現在のルール：
}

room-rules-running-from-points = { $marked ->
    [yes] 以下で変更した項目はステージ{ $number }から適用されます。●は変更した項目です。
   *[no] 以下で変更した項目はステージ{ $number }から適用されます。
}

room-rules-destination = 変更の適用先
room-rules-destination-stage = ステージ{ $number }から
room-rules-destination-next = 次の対戦

room-rules-stage-only-note-points = ステージ数とCPUは対戦中には変更できません。変更するには「次の対戦」を選ぶか、「適用してやり直す」を使ってください。
room-rules-points-shift = ステージ{ $number }より前のステージはポイント{ $before ->
    [medium] 中くらい
    [mild] 弱め
   *[strong] 強め
}のままなので、対戦の合計は配分の異なるステージを足したものになります。{ $restart ->
    [yes] 接戦にするなら「適用してやり直す」を使ってください。
   *[no] {""}
}

room-status-rules-change = ステージ{ $number }から：{ $rules }。{ $then }
room-status-then-rules-change = { $then } · ステージ{ $number }からは：{ $rules }

room-rules-change-timing = { $difficulty ->
    [easy] イージータイミング
    [hard] ハードタイミング
   *[normal] ノーマルタイミング
}
room-rules-change-boos = { $boos ->
    [on] Booあり
   *[off] Booなし
}
room-rules-change-attacks = { $attacks ->
    [on] 攻撃あり
   *[off] 攻撃なし
}
room-rules-change-at-once = { $at_once ->
    [one] 同時攻撃 1
    [two] 同時攻撃 2
    [no_limit] 同時攻撃 制限なし
   *[three] 同時攻撃 3
}
room-rules-change-shocks = { $shocks ->
    [two] 攻撃ごとのショック 2
    [six] 攻撃ごとのショック 6
    [eight] 攻撃ごとのショック 8
   *[four] 攻撃ごとのショック 4
}
room-rules-change-charts = { $charts ->
    [adaptive] 適応譜面
   *[as_picked] 選んだ譜面のまま
}
room-rules-change-points = { $points ->
    [medium] ポイント中くらい
    [mild] ポイント弱め
   *[strong] ポイント強め
}
room-rules-change-join = { $first } · { $second }

room-refused-rules-later = この変更の対象だったステージはすでに始まっています。適用先を確かめて、もう一度適用してください。

room-refused-rules-no-stage = この対戦で変更できるステージは残っていません。変更は次の対戦に適用できます。

room-table-rules-changed = ● ルール変更：{ $rules }

room-results-rules-changed = ● ステージ{ $number }から：{ $rules }

## More.

room-leave = ルームを出る

## The room's host.

room-host-options = ホスト設定

room-host-leaves = ホストが退出したとき

room-host-leaves-pass = 引き継ぐ
room-host-leaves-close = ルームを閉じる

room-host-leaves-pass-about = ホストが退出すると、共同ホスト、いなければ最初に参加したプレイヤーがホストになります。
room-host-leaves-close-about = ホストが退出すると、ルームは閉じられ、全員が退出します。

room-host-people = ほかのプレイヤー

room-host-nobody = まだほかに誰もいません。

room-make-co-host = 共同ホストにする
room-let-play = プレイを許可
room-host-watch-only = 観戦専用リンク
room-remove-co-host = 共同ホストから外す
room-make-host = ホストにする

room-make-host-title = { $name }をホストにしますか？

room-make-host-body = { $name }がルームを管理し、あなたは共同ホストになります。もう一度ホストになるには、新しいホストに戻してもらう必要があります。

## Asking before leaving a room.

room-leave-title = ルームを出ますか？

room-leave-stay = 残る

room-leave-confirm = 出る

room-leave-host-pass = このルームのホストです。{ $name }がホストになります。

room-leave-host-pass-match = このルームのホストです。{ $name }がホストになり、対戦は退出後も続きます。

room-leave-host-close = このルームのホストです。ルームは閉じられ、全員が退出します。

room-leave-host-close-match = このルームのホストです。ルームは閉じられて全員が退出し、進行中の対戦は終了します。

room-leave-host-alone = ルームにはほかに誰もいません。ルームは閉じられます。

room-leave-playing = 進行中の対戦に参加しています。対戦は退出後も続き、参加しなかったステージのポイントは0になります。

room-leave-picking = 選曲を担当しています。代わりにホストが選曲し、対戦は退出後も続きます。

room-leave-co-host = 共同ホストを務めています。進行中の対戦は退出後も続きます。

## A stage's prompt, results and the room between stages.

room-prompt-stage-cpus = { $stages ->
    [0] ステージ{ $number }
   *[other] ステージ{ $number } / { $stages }
} · CPU{ $cpus }人と対戦

room-prompt-stage-people = { $stages ->
    [0] ステージ{ $number }
   *[other] ステージ{ $number } / { $stages }
} · ほかのプレイヤー{ $others }人と対戦

room-prompt-stage-mixed = { $stages ->
    [0] ステージ{ $number }
   *[other] ステージ{ $number } / { $stages }
} · ほかのプレイヤー{ $others }人とCPU{ $cpus }人と対戦

room-prompt-life-out = ライフが尽きるとアウト
room-prompt-life-play-on = ライフが尽きると失敗のままプレイ続行

room-prompt-chart-adapts = 譜面は実力に合わせて変わる

room-prompt-stage-watched = { $stages ->
    [0] ステージ{ $number }
   *[other] ステージ{ $number } / { $stages }
} · { $players }人のプレイを観戦

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
}{ $points ->
    [medium] {" "}· ポイント中くらい
    [mild] {" "}· ポイント弱め
   *[strong] {""}
}{ $charts ->
    [as_picked] {" "}· 選んだ譜面のまま
   *[adaptive] {""}
}{ $life_out ->
    [out] {" "}· ライフ切れでアウト
   *[play_on] {""}
}{ $at_once ->
    [one] {" "}· 同時攻撃 1
    [two] {" "}· 同時攻撃 2
    [no_limit] {" "}· 同時攻撃 制限なし
   *[three] {""}
}{ $shocks ->
    [two] {" "}· 攻撃ごとのショック 2
    [six] {" "}· 攻撃ごとのショック 6
    [eight] {" "}· 攻撃ごとのショック 8
   *[four] {""}
}{ $stages ->
    [open] {" "}· ステージ数オープン
   *[counted] {""}
}{ $joining ->
    [off] {" "}· 対戦途中の参加なし
   *[on] {""}
}{ $choice ->
    [group] {" "}· グループで曲を決める
   *[host] {""}
}{ $repeats ->
    [not_in_a_row] {" "}· 連続の繰り返し不可
    [not_in_a_match] {" "}· 対戦中の繰り返し不可
   *[allowed] {""}
}

room-mark-survived = ステージ{ $number }：{ $field }人中{ $place }位 · +{ $points }pt

room-mark-out = ステージ{ $number }：ライフ切れ · { $field }人中{ $place }位 · +{ $points }pt
room-mark-failed = ステージ{ $number }：失敗（最後までプレイ） · { $field }人中{ $place }位 · +{ $points }pt

room-mark-forfeit = ステージ{ $number }：途中退出 · { $field }人中{ $place }位 · ポイントなし

room-rank-points = { $rank }位 · { $points }pt

room-points-value = { $points }

room-results-title = 対戦結果

room-results-stopped = 対戦中止 · 終わったステージのみ

room-results-winners = 1位：{ $names }

room-results-total = 合計

room-results-stage = S{ $number }

room-results-none = —

## A stage's play.

room-play-out = アウト
room-play-failed = 失敗
room-play-failed-on = 失敗：プレイ続行

room-play-won = 1位確定

room-play-last-standing = 最後の一人

room-play-ladder = { $field }人中 { $place }位 · 残り{ $left }人

room-play-ladder-field = { $field }人中 残り{ $left }人

room-play-ladder-page = { $page }/{ $pages }ページ · { $field }人中 残り{ $left }人
room-play-cue-match-begin = 対戦スタート
room-play-cue-stage-begin = ステージ{ $number }スタート
room-play-cue-final-stage = ファイナルステージ
room-play-cue-life-critical = ライフ危険
room-play-cue-survivors = 残り{ $count }人
room-play-cue-eliminated = アウト
room-play-cue-step-down = 譜面ダウン：{ $chart }
room-play-cue-step-up = 譜面アップ：{ $chart }
room-play-cue-stay = { $chart }のまま
room-play-cue-stage-drawn = 1位タイ
room-play-cue-attack-incoming = 攻撃接近
room-play-cue-attack-launched = 攻撃発動

room-attack-policy-spread = 均等
room-attack-policy-retaliate = 仕返し
room-attack-policy-leader = 最高スコア
room-play-attack-waiting = 攻撃先を待機中
room-targeting = ターゲット
room-targeting-during-play = プレイ中に切り替え
room-attack-policy-spread-about = 攻撃された回数が最も少ない相手を攻撃します。
room-attack-policy-retaliate-about = 最後に自分を攻撃した相手を攻撃します。
room-attack-policy-leader-about = スコアが最も高い相手を攻撃します。
room-results-drawn-final-attack = 最後の攻撃で1位タイ
room-prompt-speed-raised = 攻撃の予告が間に合うようスピードを上げています

## The footer's key hints.

room-hint-move = 移動
room-hint-back = 戻る

## Online rooms.

room-account-signed-in = { $name }でサインイン中

room-account-signed-out = サインインしていません。CPU練習にアカウントは不要です。

room-account-checking = サインイン状態を確認しています…

room-account-unreachable = サーバーに接続できませんでした。CPU練習は使えます。

room-sign-in = Twitchでサインイン
room-sign-out = サインアウト

room-invite-field = 招待リンク
room-invite-field-hint = 招待リンクを貼り付け

room-invite-go = 招待を開く

room-invite-not-one = 招待リンクではありません。
room-invite-room-address = これはルームのページのアドレスで、招待リンクではありません。ホストに招待リンクをもらってください。

room-sign-in-waiting = ブラウザでTwitchのサインインを済ませてから、ここに戻ってください。

room-sign-in-code-type = ブラウザで開いたページに、このコードを入力してください。
room-sign-in-code-copy = コピー
room-sign-in-code-copied = コピーしました

room-sign-in-too-many-codes = ブラウザで間違ったコードが何度も入力されたため、サインインは終了しました。「もう一度」で新しいコードが表示されます。

room-sign-in-again-browser = もう一度ブラウザを開く
room-sign-in-cancel = キャンセル

room-sign-in-completing = サインインしています…

room-sign-in-canceled = サインインをキャンセルしました。

room-sign-in-failed = サインインできませんでした（{ $code }）。

room-sign-in-retry = もう一度

room-creating = ルームを作成しています…

room-create-failed = ルームを作成できませんでした（{ $code }）。

room-create-paused = 新しいルームの作成は現在停止中です。しばらくしてからもう一度お試しください。

room-create-busy = サーバーのルーム数が上限に達しています。しばらくしてからもう一度お試しください。

room-create-too-many = 同時にホストできるルーム数の上限に達しています。どれかを閉じるか、しばらくしてからもう一度お試しください。

## An invite's page.

room-invite-title = 招待

room-invite-heading = 招待

room-invite-reading = 招待を確認しています…

room-invite-host = ホスト：{ $name }

room-invite-members = ルームに{ $count }人

room-invite-open = いま参加できます。「準備OK」は自分で押します。
room-invite-next-match = 対戦中です。今の対戦は観戦し、次の対戦から参加します。
room-invite-next-song = 対戦中です。次の曲から参加できます。
room-invite-closed = ホストがこのルームへの新しい参加を締め切りました。
room-invite-full = このルームは満員です。
room-invite-full-watch = プレイヤーの席が埋まっています。観戦として参加します。
room-invite-watch = 観戦に招待されています。

room-invite-expired = この招待は期限切れです。ホストに新しいリンクをもらってください。
room-invite-rotated = この招待は差し替えられました。ホストに新しいリンクをもらってください。
room-invite-unknown = この招待は存在しません。
room-invite-room-closed = このルームは閉じられました。
room-invite-unreachable = サーバーに接続できませんでした。

room-invite-refused = 参加できませんでした（{ $code }）。

room-invite-join = ルームに参加
room-invite-watch-join = 観戦する
room-invite-sign-in = Twitchでサインインして参加

room-invite-joining = 参加しています…

## The online room.

room-link-joining = ルームに接続しています…

room-link-reconnecting = 接続が切れました。再接続しています…

room-link-sign-in-again = サインインの期限が切れました。ルームに戻るにはもう一度サインインしてください。

room-link-closed = このルームは閉じられました。
room-link-opened-elsewhere = このルームは別のデバイスで開かれました。
room-opened-elsewhere-body = このルームは、ほかのデバイスやタブ4つですでに開かれています。「このデバイスを使う」でここでプレイすると、接続がいちばん長いデバイスが代わりに止まります。
room-use-this-device = このデバイスを使う
room-opened-elsewhere-not-now = 今はしない
room-closed-by-operator = ルームは閉じられました。
room-update-required = このルームには新しいバージョンのゲームが必要です。
room-update-reload = ゲームが更新されました。このルームで遊ぶには再読み込みしてください。
room-update-reload-button = 再読み込み
room-server-updating = サーバーを更新しています。しばらくしてからもう一度お試しください。

room-link-left = このルームから退出しました。

room-ended-body = マルチプレイのホームに戻ります。
room-link-sign-in-to-enter = このルームに入るにはサインインしてください。
room-sign-in-to-enter-body = ルームには招待リンクから入ります。すでにこのルームのメンバーなら、Twitchでサインインすると戻れます。そうでなければ、ホストに招待リンクをもらってください。
room-link-not-a-member = このルームのメンバーではありません。ホストに招待リンクをもらってください。
room-not-a-member-body = ルームのページのアドレスでは入れません。入るには招待リンクが必要です。ホストに送ってもらってください（ルームの「その他」→「招待リンクをコピー」）。

room-role-host = ホスト
room-role-co-host = 共同ホスト
room-role-admin = 管理者
room-watching = 観戦
room-next-match = 次の対戦から
room-next-song = 次の曲から
room-place-full = 空席なし
room-joined-at = ステージ{ $number }から

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

room-chart-no-digest = この譜面はまだオンラインで選べません。ライブラリーの読み込み後にもう一度お試しください。

room-copy-invite = 招待リンクをコピー
room-copy-watch-invite = 観戦専用リンクをコピー
room-replace-invite = 招待リンクを差し替え
room-replace-invite-title = 招待リンクを差し替えますか？
room-replace-invite-body = 古い招待リンクは使えなくなり、そこから作られたほかのリンク（観戦専用と観客）も使えなくなります。すでにルームにいる人はそのまま残ります。
room-sign-in-again = もう一度サインイン

room-invite-copied = 招待リンクをコピーしました。ルームは誰かがいる間だけ開いています。
room-watch-invite-copied = 観戦専用リンクをコピーしました。
room-invite-replaced = 招待を差し替えました。古いリンクは使えません。
room-invite-not-replaced = 招待を差し替えられませんでした。
room-joined-to-watch = プレイヤーの席が埋まっているため、観戦しています。席が空いたら「プレイする」を選べます。

## The match flow (M7b).

room-status-pick-first = 曲を選んで対戦を始めましょう。
room-status-alone-invite = 曲を選んで対戦を始めましょう。人を呼ぶには、このページのアドレスではなく「その他」から招待リンクを共有してください。

room-status-pick-next = ステージ{ $number }の曲を選んでください。

room-status-gathering = 全員の準備ができたらステージ{ $number }開始 · { $seconds }秒

room-status-loading-wait = { $count }人の読み込みを待っています · { $seconds }秒

room-status-retry-wait = { $more ->
    [0] { $name }を待っています：ダウンロードを再試行中 · { $seconds }秒
   *[other] { $name }ほか{ $more }人を待っています：ダウンロードを再試行中 · { $seconds }秒
}
room-status-sitting-out = ステージ{ $number }は見学です。ほかのプレイヤーがプレイしています。{ $then }

room-status-sat-out-not-ready = ステージ{ $number }の開始時に準備OKになっていませんでした。{ $then }
room-status-sat-out-loading = ステージ{ $number }の開始時に、このデバイスで曲を読み込み中でした。{ $then }
room-status-sat-out-failed = ステージ{ $number }の曲をこのデバイスで読み込めませんでした。{ $then }
room-status-sat-out-away = ステージ{ $number }の開始時に接続が切れていました。{ $then }

room-sat-out-then-watch-next = このステージを観戦し、次のステージからプレイします。
room-sat-out-then-watch-last = 対戦最後のこのステージを観戦します。
room-sat-out-then-watch-out = このステージを観戦します。欠場したため、この対戦には戻れません。
room-sat-out-then-watch-chosen = このステージを観戦します。「プレイする」を選ぶと次のステージからプレイできます。
room-sat-out-then-next = 次のステージからプレイします。
room-sat-out-then-last = この対戦の最後のステージです。
room-sat-out-then-out = 欠場したため、この対戦には戻れません。
room-sat-out-then-chosen = 「プレイする」を選ぶと次のステージからプレイできます。

room-status-waiting-next-match = ステージ{ $number }をプレイ中です。対戦の途中で参加したため、次の対戦からプレイします。
room-status-waiting-next-stage = ステージ{ $number }をプレイ中です。対戦の途中で参加したため、次の曲からプレイします。
room-status-waiting-place = この対戦の席はCPUの分も含めてすべて埋まっています。次の対戦から、または席が空けばその時点の次の曲からプレイします。
room-status-last-song = 最後の曲 · { $line }

room-status-pick-new-match = 対戦終了。曲を選んで次の対戦を始めましょう。

room-status-stopped-pick = 対戦を中止しました。曲を選んで次の対戦を始めましょう。

room-status-stopped-new-rules = 対戦を中止しました。曲を選んで、新しいルールで次の対戦を始めましょう。

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
room-cpu-style-stream = ストリーム
room-cpu-style-jacks = 縦連
room-cpu-style-chords = 同時押し
room-cpu-style-stamina = スタミナ
room-cpu-style-technical = テクニカル
room-cpu-style-evasive = 回避上手
room-cpu-style-reckless = 踏みがち
room-cpu-style-custom = カスタム

room-state-loading = 読み込み中
room-state-retrying = ダウンロード再試行中
room-state-loading-percent = 読み込み中 { $percent }%
room-state-load-failed = 読み込み失敗
room-state-sitting-out = 見学
room-state-waiting = 曲を待機中
room-state-sat-out-not-ready = 見学：未準備
room-state-sat-out-loading = 見学：読み込み中
room-state-sat-out-failed = 見学：読み込み失敗
room-state-sat-out-away = 見学：離席
room-state-failed-download = ダウンロード失敗
room-state-failed-unreadable = 曲を読めません
room-state-failed-changed = 譜面が変更されました
room-state-failed-plan = 攻撃が不一致

room-strip-more = +{ $count }

room-mark-short = { $field }人中{ $place }位 · +{ $points }pt
room-mark-short-forfeit = { $field }人中{ $place }位 · 途中退出

room-picks-heading = 選曲

room-picks-count = { $stages ->
    [0] { $picked }曲
   *[other] { $stages }曲中 { $picked }曲
}

room-panel-more-above = ▲ ほか{ $count }件
room-panel-more-below = ▼ ほか{ $count }件
room-panel-fold = プレイヤー欄をたたむ
room-panel-unfold = プレイヤー欄を開く

room-refused-too-few = ステージを始めるにはプレイヤーが足りません。
room-refused-host-only = ホストだけが行えます。
room-refused-title = できません

room-rules-ok = OK

room-rules-locked-line = { $stages ->
    [0] ステージ数オープン
   *[other] { $stages }ステージ
} · CPU{ $cpus }人 · { $rules }

room-rules-no-cpus = CPUの対戦相手はいません。

room-rules-remove = ×

room-rules-add-random = ランダムに追加
room-rules-add-custom = 選んで追加…

room-add-cpus-title = CPUを追加

room-add-cpus-count = 人数

room-add-cpus-add = CPUを{ $count }人追加

room-add-cpus-by-name = または名前で1人

room-add-cpus-full = 席はすべて埋まっています。

room-add-cpus-none-left = すべてのCPUが参加済みです。

room-standings-title = ステージ{ $number }の順位

room-standings-continue = 次へ

room-prompt-canceled = ステージの開始は取り消されました。

room-prompt-unready-hint = 長押しまたは2回押しで取り消し

room-prompt-starting = まもなく開始…

room-prompt-leave-hint = 長押しまたは2回押しでステージをやめる

room-prompt-footer-unready = 準備を取り消す：長押しまたは2回押し
room-prompt-footer-leave = やめる：長押しまたは2回押し

room-due-title = ステージ{ $number }が始まります
room-due-play = ルームの次のステージが始まります。準備画面でプレイに参加できます。
room-due-watch = プレイしないステージが始まります。観戦できます。
room-due-go = 準備画面へ
room-due-go-watch = 観戦する
room-due-stay = このまま

room-results-tab-standings = 順位
room-results-tab-graphs = グラフ

room-results-hint-tabs = タブ切り替え

room-table-title = { $stages ->
    [0] ステージ{ $number }
   *[other] ステージ{ $number } / { $stages }
}

room-table-waiting = 全員の結果を待っています…

room-table-player = プレイヤー
room-table-award = ステージ
room-table-total = 合計
room-table-match = 総合順位
room-table-score = スコア
room-table-grade = 評価
room-table-life = ライフ
room-table-combo = コンボ
room-table-judgments = 判定

room-table-award-value = +{ $points }

room-table-playing = プレイ中…

room-table-out = アウト { $time }
room-table-failed = 失敗 { $time }

room-table-left = 退出
room-table-stepped = ↓ { $chart }
room-table-stepped-up = ↑ { $chart }
room-table-started-moved = { $start } → { $chart }

room-table-sent = 発動

room-table-taken = 被攻撃

room-table-shocks = ショック

room-summary-sent = 攻撃した相手：{ $list }

room-summary-sent-none = 攻撃した相手なし

room-summary-taken = 攻撃してきた相手：{ $list }

room-summary-taken-none = 攻撃してきた相手なし

room-summary-shocks = 踏んだショック：{ $count }

room-final-heading = 対戦結果

## 観戦

room-intent-watch = 観戦のみ

room-intent-play = プレイする

room-refused-all-watching = 全員が観戦中です。誰かがプレイする必要があります。
room-refused-players-full = プレイヤーの席が埋まっています。席が空いてから「プレイする」を選んでください。
room-refused-watch-only = 観戦専用リンクで参加しています。ホストか共同ホストが許可するとプレイできます。

room-watch-hint = Tabかフィールドのタップで1人を観戦

room-watch-hint-pages = Tabかタップで1人を観戦 · PageUp/Downか十字キーでほかのプレイヤー
room-live-lost = ルームでステージのライブ中継が途切れました。結果は終了時に集計されます

room-watch-following = { $name }を観戦中

room-watch-next = 次のプレイヤー

room-watch-place = { $field }人中{ $place }位

room-watch-cue-out = { $name }がアウト

room-watch-cue-won = { $name }が1位確定

room-watch-cue-last-standing = { $name }が最後の一人

room-watch-cue-attack = { $from }が{ $to }を攻撃

room-watch-tap-hint = フィールドをタップでそのプレイヤーを観戦

room-play-forfeit = 途中退出

room-play-skip = { $key }：結果へ

room-play-skip-tap = 結果へ

room-leave-forfeited = このステージを途中で退出しました。ポイントはありません。

room-watched-winner = ステージ1位

room-watched-winners = ステージ1位

room-watched-note = このステージは観戦しました。

room-audience-watching = { $count }人が観戦中
room-audience-closed-short = 観客席は閉鎖中
room-status-with-audience = { $status } · { $count }人が観戦中
room-copy-audience-invite = 観客用リンクをコピー
room-audience-invite-copied = 観客用リンクをコピーしました。誰でも観戦できます。
room-host-audience = 観客席
room-host-audience-line = 観客席 · { $state }
room-audience-close = 観客席を閉じる
room-audience-open = 観客席を開く
room-audience-closed-notice = 観客席を閉じました。いまは誰も観戦していません。
room-audience-opened-notice = 観客席をまた開きました。

room-invite-audience = 観客として観戦に招待されています。
room-invite-full-audience = このルームは満員です。観客として観戦できます。
room-invite-audience-join = 観客として観戦
room-audience-joining = 観客席に入っています…

room-audience-full = このルームの観客席は満員です。
room-audience-closed = ホストが観客席を閉じました。
room-audience-too-many-seats = すでに2台のデバイスでこのルームを観戦しています。
room-audience-too-many-network = お使いのネットワークからこのルームを観戦している人が多すぎます。
room-audience-busy = お使いのネットワークから短時間に多くの参加がありました。しばらくしてからもう一度お試しください。
room-audience-busy-account = 短時間に何度もこのルームの観客席に参加しました。しばらくしてからもう一度お試しください。
room-audience-link-unknown = この観客用リンクはこのルームのものではないか、差し替えられました。
room-audience-ended-back = { $key }：戻る
room-audience-ended-back-tap = 戻る
room-audience-none = このルームには観客席がありません。

room-audience-title = 観戦
room-audience-heading = 観客席
room-audience-leave = 観戦をやめる
room-audience-watch-stage = ステージを観戦
room-audience-doing-lobby = ホストが最初の曲を選んでいます。
room-audience-doing-picking = ステージ{ $number }：曲を選んでいます。
room-audience-doing-picked = ステージ{ $number }：{ $song }。プレイヤーの準備ができしだい始まります。
room-audience-doing-not-live = ステージ{ $number }をプレイ中です。このルームのステージは観戦できません。結果はここに届きます。
room-audience-doing-match-over = 対戦が終わりました。
room-audience-doing-stopped = 対戦は中止されました。
room-audience-plan-mismatch = このデバイスで計算したステージの攻撃がルームと一致しないため、ショックなしで表示します。
room-audience-song-unreadable = ステージの曲をここで読めませんでした。結果はステージの終わりに届きます。
room-audience-stage-won = ステージ{ $number }の1位：{ $names }
room-audience-standings = 順位
room-audience-standing = { $rank }. { $name } · { $points }pt
room-audience-players = プレイヤー{ $count }人

room-refused-rules-changed = その間にほかの人がルールを変更しました。今のルールを表示しています。確かめてから、もう一度適用してください。

## The room's song list (room model revision 17).

room-tab-room-songs = ルームの曲
room-tab-my-songs = 自分の曲
room-songs-empty-adder = ルームにはまだ曲がありません。「自分の曲」から追加してください。
room-songs-empty-member = ホストはまだ曲を追加していません。
room-song-uploading = アップロード中
room-song-uploading-percent = アップロード中 { $percent }%
room-song-failed = アップロード失敗
room-song-ready = 取得済み
room-song-loading = 読み込み中
room-song-not-loaded = 未読み込み
room-song-downloading-percent = ダウンロード中 { $percent }%
room-song-downloading = ダウンロード中
room-song-state-have = { $state } · { $of }人中{ $have }人が所持
room-song-added-by-you = 自分が追加
room-song-added-by = { $name }が追加
room-song-line = { $artist } · { $by } · { $state }
room-song-line-no-artist = { $by } · { $state }
room-song-line-spotlight = { $picker }が選択中 · { $artist } · { $by } · { $state }
room-add-song-use = ルームに追加
room-song-in-room = ルームにあります
room-remove-song = ルームから削除
room-retry-upload = もう一度アップロード
room-refused-song-listed = その曲はすでにルームにあります。
room-refused-list-full = ルームの曲リストがいっぱいです。
room-refused-too-many-songs = このルームに追加できる{ $limit }曲をすでに追加しています。追加するには自分の曲を削除してください。
room-refused-hosts-add = 今このルームに曲を追加できるのはホストだけです。
room-refused-song-in-use = その曲はステージに選ばれているか、プレイヤーが選択しているため、今は残ります。
room-refused-song-not-ready = その曲はまだアップロード中です。
room-refused-song-gone = その曲はもうルームのリストにありません。
room-refused-song-invalid = その曲はルームに追加できません。

## Host options: the room's song list.

room-host-list-size = ルームの曲の上限
room-host-list-size-value = { $count }曲
room-host-when-full = リストがいっぱいのとき
room-host-when-full-remove = 古い曲を削除
room-host-when-full-refuse = 新しい曲を断る
room-host-who-adds = 曲を追加できる人
room-host-who-adds-hosts = ホスト
room-host-who-adds-everyone = 全員
room-host-list-note = ホストと共同ホストはリストいっぱいまで、それ以外の人は{ $count }曲まで追加でき、自分の曲を削除できます。曲は削除されるかルームが終わるまで残ります。
room-host-list-note-hosts = 曲を追加できるのはホストと共同ホストだけで、リストいっぱいまで追加できます。曲は削除されるかルームが終わるまで残ります。

room-ahead-quota = このブラウザにルームの曲を保存する空きがなくなりました。これからは次に遊びそうな曲だけを先にダウンロードします。
room-status-my-songs-picker = ここで曲をルームに追加できます。次の曲は「ルームの曲」で選んでください。
room-status-my-songs-adder = ここで曲をルームに追加できます。
room-status-my-songs-count = ここで曲をルームに追加できます。追加した曲：{ $limit }曲中{ $added }曲。
room-status-my-songs-at-limit = 追加できる{ $limit }曲をすべて追加しました。追加するには自分の曲を削除してください。
room-status-my-songs-full = ルームの曲リストがいっぱいです。先に曲を削除する必要があります。
room-add-pack = このパックをルームに追加（{ $count }曲）
room-status-adding-pack = パックをルームに追加中：{ $count }曲中{ $done }曲をアップロード済み。

room-moderate = { $on ->
    [1] 管理モード：オン
   *[other] 管理モード：オフ
}
room-kick = 退出させる
room-kicked-someone = { $by }が{ $name }をルームから退出させました。
room-link-kicked = ホストによってルームから退出させられました。
room-kicked-body = 招待リンクからこのルームに戻ることができます。
room-link-not-admitted = このルームには参加できません。
room-not-admitted-body = ほかのルームには、それぞれの招待リンクから参加できます。
room-host-twitch-bans = Twitchのバン
room-host-twitch-off = オフ
room-host-twitch-bans-only = バン
room-host-twitch-bans-blocks = バンとブロック
room-host-twitch-chat = 自分のTwitchチャットを表示
room-host-twitch-emotes = エモート
room-host-twitch-messages = エモートとメッセージ
room-host-twitch-whose = { $login }のTwitchのバンとチャットが使われます。
room-host-twitch-chat-about = 自分のTwitchチャットを表示：各ステージの間、自分のチャンネルのチャットがフィールドの後ろを流れます。表示されるのは、自分のデバイスで「ルームでのTwitchチャット」をオンにしたプレイヤーだけです（オプション → 表示）。
room-host-twitch-remembered = 次に作るルームは、この2つをここで最後に選んだ設定で始まります。
room-twitch-chat-no-access = ホストのチャットを見るには、Twitchで許可してください。
room-twitch-chat-allow = Twitchで許可
room-twitch-chat-allow-more = Twitchチャットを許可
room-chat-trouble-access = Twitchチャットを表示できませんでした：Twitchアクセスがチャットの読み取りを許可していません。ルームの「その他」→「Twitchチャットを許可」でTwitchに許可を求められます。
room-chat-trouble-refused = Twitchチャットを表示できませんでした：Twitchがこのアカウントにチャンネルのチャットを許可しませんでした。
room-chat-trouble-gone = Twitchチャットを表示できませんでした：チャンネルがTwitchにありません。
room-chat-trouble-unreachable = Twitchチャットを表示できませんでした：Twitchへの接続に失敗しました。
room-chat-trouble-not-allowed = Twitchチャットを表示できませんでした：ほかのチャンネルのチャットを表示できるのは管理者だけです。
room-twitch-not-connected = Twitchアクセスは接続されませんでした。何も変わっていません。
room-twitch-bans-off = Twitchで許可されなかったため、Twitchのバンはオフです。
room-twitch-chat-on = このデバイスでルームのTwitchチャットを表示します。
room-twitch-connecting = Twitchアクセスを接続しています…
room-twitch-connect-waiting = ブラウザでTwitchアクセスを許可してから、ここに戻ってください。
room-host-trip-why = 新しいルームはTwitchのバンがオンで始まります。チャンネルでバンやタイムアウトされた人をゲームが確認することを許可してください。許可しない場合は、オフのままルームが作られます。
room-twitch-ended = Twitchアクセスが終了したため、Twitchのバンはオフです。
room-twitch-bans-allow = Twitchで許可
room-twitch-bans-allow-more = Twitchのバンを許可
room-twitch-bans-off-by = { $name }のTwitchのバンはオフになりました。
room-twitch-setter-left = { $name }が退出したため、そのTwitchのバンとチャットはオフになりました。
room-rules-choice = 曲の決め方
room-rules-choice-host = ホスト
room-rules-choice-group = グループ
room-rules-choice-host-about = ホストがルームの曲リストから曲を選びます。
room-rules-choice-group-about = プレイヤーがそれぞれルームのリストから曲を選びます。ホストか共同ホストが自分で選ぶと10秒のカウントダウンが始まり、その後、誰か一人の選んだ曲が抽選で決まります。
room-rules-repeats = 繰り返し
room-rules-repeats-allowed = 可
room-rules-repeats-not-in-a-row = 連続は不可
room-rules-repeats-not-in-a-match = 対戦中1回
room-rules-repeats-about = { $repeats ->
    [not_in_a_row] 直前にプレイした曲は次のステージに選べません。選べる曲がなくなる場合は、どの曲でも選べます。
    [not_in_a_match] 各曲は1回の対戦につき1回までです。この対戦でプレイした曲は、次の対戦まで選べません。選べる曲がなくなる場合は、どの曲でも選べます。
   *[allowed] 直前にプレイした曲も含め、どの曲でも選べます。
}
room-rules-change-choice = { $choice ->
    [group] グループで曲を決める
   *[host] ホストが曲を決める
}
room-rules-change-repeats = { $repeats ->
    [not_in_a_row] 連続の繰り返し不可
    [not_in_a_match] 対戦中の繰り返し不可
   *[allowed] 繰り返し可
}
room-status-group-none-host = ステージ{ $number }の曲を選んでください。選ぶと抽選が始まります。
room-status-group-none = ステージ{ $number }の曲を選んでください。ホストか共同ホストが選ぶと抽選が始まります。
room-status-group-some-host = { $of }人中{ $chose }人がステージ{ $number }の曲を選びました。選ぶと抽選が始まります。
room-status-group-some = { $of }人中{ $chose }人がステージ{ $number }の曲を選びました。ホストか共同ホストが選ぶと抽選が始まります。
room-status-group-waits-host = { $of }人中{ $chose }人がステージ{ $number }の曲を選びました。自分の選択は待機中です。選んだ曲でEnterキーを押すかタップすると抽選が始まります。
room-status-group-waits = { $of }人中{ $chose }人がステージ{ $number }の曲を選びました。ホストか共同ホストが開始すると抽選が始まります。
room-status-group-watching = { $of }人中{ $chose }人のプレイヤーがステージ{ $number }の曲を選びました。曲を選ぶのはプレイヤーだけです。
room-status-draw = 抽選まで{ $seconds }秒 · { $of }人中{ $chose }人が選択 · { $name }が開始
room-status-draw-waits = { $why ->
    [rules] 抽選は待機中です。先にホストが次の対戦のルールを確認する必要があります。
   *[players] ステージを始められるまで抽選は待機します。プレイヤーがもっと必要です。
}
room-status-group-no-starter = { $of }人中{ $chose }人がステージ{ $number }の曲を選びました。抽選にはプレイするホストか共同ホストが必要です。
room-status-group-watching-host = { $cohost ->
    [yes] 観戦中です。抽選を始めるには、プレイに切り替えて曲を選ぶか、共同ホストに頼んでください。
   *[no] 観戦中です。抽選を始めるには、プレイに切り替えて曲を選んでください。
}
room-choice-gone = { $why ->
    [host] 曲の選択が取り消されました：曲はホストが選ぶようになりました。
    [repeats] 曲の選択が取り消されました：繰り返しのルールでその曲は選べなくなりました。
    [song] 曲の選択が取り消されました：その曲はもうプレイできません。
   *[stage] 曲の選択が取り消されました：抽選は中止されました。
}
room-draw-by-you = 自分
room-song-line-chosen = { $chosen } · { $line }
room-song-chosen-one = { $a }が選択
room-song-chosen-two = { $a }と{ $b }が選択
room-song-chosen-more = { $a }とほか{ $count }人が選択
room-chosen-you = 自分
room-song-line-repeat = { $rule ->
    [not_in_a_row] 前のステージの曲
   *[not_in_a_match] この対戦で選曲済み
} · { $line }
room-hint-choose = この曲を選ぶ
room-hint-choose-draw = この曲を選んで抽選開始
room-hint-clear-choice = 選択を取り消す
room-hint-start-draw = 抽選を開始
room-hint-repeated = { $rule ->
    [not_in_a_row] 選べません：前のステージの曲
   *[not_in_a_match] 選べません：この対戦で選曲済み
}
room-refused-not-group = 次の曲はホストが選びます（曲の決め方：ホスト）。
room-refused-cannot-choose = { $why ->
    [watching] 観戦中のため曲は選べません。次の曲を選ぶのはプレイヤーだけです。
    [next_match] 次の対戦から参加するため、この曲は選べません。
   *[out_for_match] この対戦から脱落したため、曲は選べません。
}
room-refused-repeated = { $rule ->
    [not_in_a_row] その曲は前のステージの曲です（繰り返し：連続は不可）。
   *[not_in_a_match] その曲はこの対戦ですでに選ばれています（繰り返し：対戦中1回）。
}
room-refused-group-chooses = 次の曲はプレイヤーが選び、抽選で決まります（曲の決め方：グループ）。
room-role-chose = 曲を選択済み
room-audience-chose = { $title }を選択
room-draw-heading = ステージ{ $number } · 抽選
room-draw-won = { $name }の選択：{ $title } · { $chart }
room-draw-won-you = 自分の選択：{ $title } · { $chart }
room-draw-entry = { $name } · { $title } · { $chart }
room-draw-entry-you = 自分
room-draw-won-song = { $name }の選択：{ $title }
room-draw-won-you-song = 自分の選択：{ $title }
room-draw-entry-song = { $name } · { $title }
