## The pages to read: calibration's menu, the credits, desktop's offer of
## other games' song folders.

pages-move-hint = 移動

pages-activate = 決定

pages-press-to-open = { $link } · Enterキーかタップで開きます

## Calibration's menu.

pages-calibration-title = キャリブレーション

pages-calibration-heading = キャリブレーション

pages-calibration-last-test = 前回のテスト

pages-calibration-about = 各テストでは、専用の譜面（100 BPM、クリック音3回と休みの繰り返し、ノーツごとにクリック音）を通常の曲と同じようにプレイします。入力がどれだけ早いか遅いかを測り、プレイ中に補正します。残りの誤差がばらつきの範囲に収まるまで測定し（信頼水準95%）、自動で止まります。保存すると、補正値が設定に加わります。

pages-calibration-order = 2つのオフセットは独立しています。ガイドでは、矢印のみ（表示）、音のみ（音声経路）の順に測り、最後に両方で確認します。最後のずれはほぼゼロになるはずです。

pages-calibration-range = ±{ $ms } msを超えるオフセットはこの方法では測れません（それほど遅い入力は、次のノーツへの入力と区別できないためです）。その場合は先にオプションで手動で設定し、そこからキャリブレーションします。

pages-calibration-visual = 1. 矢印のみ

pages-calibration-visual-about = 音は鳴りません。矢印が判定ラインに重なったときに押します。表示の遅れを測り、表示オフセットに保存します（表示位置だけが変わります）。

pages-calibration-start-guided = ガイド付きキャリブレーション開始

pages-calibration-audio = 2. 音のみ

pages-calibration-audio-about = 矢印は表示されません。クリックごとにいずれかの矢印キーを押します。音声経路の遅れを測り、音声オフセット（判定のタイミングが動きます）に保存します。

pages-calibration-start = 開始

pages-calibration-combined = 3. チェック：両方

pages-calibration-combined-about = 普段どおりにプレイします。1と2の後なら、ずれはほぼゼロになるはずです。はっきりしたずれが残る場合は、音声オフセットの微調整として保存できます。

pages-calibration-offsets = オフセット

pages-calibration-defaults = プロファイルのないデバイスの既定値：音声 { $audio } ms、表示 { $visual } ms。

pages-calibration-no-device = 測定済みのデバイスはまだありません。

pages-calibration-per-device = オフセットは音声経路ごと、ディスプレイごとに保存され、曲の開始時に一致するものが使われます。

pages-calibration-audio-device = { $calibrated ->
    [yes] 音声：{ $device } — { $ms } ms（測定済み）
   *[no] 音声：{ $device } — { $ms } ms
}

pages-calibration-display-device = { $calibrated ->
    [yes] 表示：{ $device } — { $ms } ms（測定済み）
   *[no] 表示：{ $device } — { $ms } ms
}

pages-calibration-reset = オフセットを0に戻す

pages-calibration-tip = ヒント：オプションで「タイミング誤差の表示」をオンにすると、通常のプレイ中に各入力のずれが表示されます。「デバッグ表示」には、それまでの平均が表示されます。

pages-calibration-options = オプション

pages-calibration-reset-title = オフセットをリセットしますか？

pages-calibration-reset-body = 既定の音声・表示オフセットと、すべてのデバイスのオフセットが0 msに戻ります。デバイスの登録は残り、未測定の状態になります。

## What a test run came to.

pages-calibration-run-device-changed = { $test ->
    [visual] 矢印のみのテスト：途中でデバイスが変わったため、何も保存されませんでした。もう一度実行してください。
    [audio] 音のみのテスト：途中でデバイスが変わったため、何も保存されませんでした。もう一度実行してください。
   *[combined] 両方でのチェック：途中でデバイスが変わったため、何も保存されませんでした。もう一度実行してください。
}

pages-calibration-run-unreliable = { $test ->
    [visual] 矢印のみのテストは安定して測定できなかったため、何も変更していません。すべてのノーツを一定のリズムで叩いて、もう一度実行してください。
    [audio] 音のみのテストは安定して測定できなかったため、何も変更していません。すべてのノーツを一定のリズムで叩いて、もう一度実行してください。
   *[combined] 両方でのチェックは安定して測定できなかったため、何も変更していません。すべてのノーツを一定のリズムで叩いて、もう一度実行してください。
}

pages-calibration-run-out-of-range = { $test ->
    [visual] 矢印のみのテストは安定して測定できなかったため、何も変更していません。すべてのノーツを一定のリズムで叩いて、もう一度実行してください。矢印が本当にそれほど遅れて表示される場合は、先にオプションで表示オフセットを手動で設定します。
    [audio] 音のみのテストは安定して測定できなかったため、何も変更していません。すべてのノーツを一定のリズムで叩いて、もう一度実行してください。クリックが本当にそれほど遅れて聞こえる場合は、先にオプションで音声オフセットを手動で設定します。
   *[combined] 両方でのチェックは安定して測定できなかったため、何も変更していません。すべてのノーツを一定のリズムで叩いて、もう一度実行してください。クリックが本当にそれほど遅れて聞こえる場合は、先にオプションで音声オフセットを手動で設定します。
}

pages-calibration-run-right = { $test ->
    [visual] 矢印のみのテストの結果、表示オフセットは今のままで合っています。
    [audio] 音のみのテストの結果、音声オフセットは今のままで合っています。
   *[combined] 両方でのチェックの結果、音声オフセットは今のままで合っています。
}

pages-calibration-run-set = { $test ->
    [visual] 矢印のみのテストで、表示オフセットを{ $ms } msに設定しました。
    [audio] 音のみのテストで、音声オフセットを{ $ms } msに設定しました。
   *[combined] 両方でのチェックで、音声オフセットを{ $ms } msに設定しました。
}

## The credits.

pages-credits-title = クレジット

pages-credits-heading = クレジット

pages-credits-loading = 読み込み中…

pages-credits-unreadable = クレジットを読み込めませんでした：{ $error }

pages-credits-about = { -game-name }のコードはMITライセンスです。同梱のデモ曲、ノートスキン、フォント、動画デコーダー、他のプロジェクトのコードは、それぞれのライセンスのもとで使用している第三者の作品で、各ライセンスの求めに従ってここに記載しています。私たちのアナウンサーはCC0で、その制作に使ったものを記載しています。

pages-credits-source = ソース：{ $url }

pages-credits-songs = 曲
pages-credits-noteskins = ノートスキン
pages-credits-announcers = アナウンサー
pages-credits-fonts = フォント
pages-credits-video = 動画デコーダー
pages-credits-code = 他のプロジェクトのコード
pages-credits-rust = Rustライブラリー

pages-credits-commit-folder = コミット { $commit }、フォルダー「{ $folder }」

pages-credits-commit-folder-retrieved = コミット { $commit }、フォルダー「{ $folder }」、取得日 { $date }

pages-credits-chart-authors = 譜面作者：{ $authors }

pages-credits-not-included = 含まれないもの：{ $files }

pages-credits-no-modifications = 変更：なし

pages-credits-modifications = 変更：{ $changes }

pages-credits-model = モデル：{ $url }

pages-credits-revision = リビジョン { $revision }

pages-credits-revision-made = リビジョン { $revision }、作成日 { $date }

pages-credits-learned-from = 声（{ $voice }）の学習元「{ $what }」：{ $holders } — { $license }

pages-credits-source-link = ソース

pages-credits-modified = 変更：{ $how }

pages-credits-holders-commit = { $holders } · コミット { $commit }

pages-credits-notice = ライセンス表示

pages-credits-video-about = { $platform ->
    [desktop] 背景動画は、曲が必要とするときに専用のサンドボックスで動く別のモジュールで再生されます。このモジュールは{ $license }のもとでライセンスされた{ $name } { $version }のコードを使用しており、そのソースは変更なしでモジュールに同梱されています（SHA-256 { $sha256 }）。このゲームのリポジトリのvideo/build.shで下記のフラグを使ってビルドされ、ゲーム自身のコード（MIT）とリンクされています。
   *[web] ブラウザーがデコードできない背景動画は、曲が必要とするときだけ取得される別のモジュールで再生されます。このモジュールは{ $license }のもとでライセンスされた{ $name } { $version }のコードを使用しており、そのソースは変更なしでモジュールの隣で配信されています（SHA-256 { $sha256 }）。このゲームのリポジトリのvideo/build.shで下記のフラグを使ってビルドされ、ゲーム自身のコード（MIT）とリンクされています。
}

pages-credits-video-source = ソース：{ $file }

pages-credits-video-release = FFmpegでのリリース

pages-credits-video-tag = タグ

pages-credits-video-licenses = ライセンス全文

pages-credits-video-licenses-where = { $platform ->
    [desktop] （モジュールの隣にあります。モジュールを含むビルドのみ）
   *[web] （モジュールと一緒に配信されます。モジュールを含むビルドのみ）
}

pages-credits-configure = configureのフラグ

pages-credits-rust-about = { $platform ->
    [desktop] このゲームのデスクトップアプリは、{ $count }個のオープンソースのRustライブラリーを使って作られています。それらのライセンスは、ここにある表示を求めています。複数のライセンスから選べるライブラリーには、使用しているライセンスを記載しています。
   *[web] このゲームのWebアプリ（Opusデコーダーを含む）は、{ $count }個のオープンソースのRustライブラリーを使って作られています。それらのライセンスは、ここにある表示を求めています。複数のライセンスから選べるライブラリーには、使用しているライセンスを記載しています。
}

pages-credits-rust-crates = ライブラリー（{ $count }）

pages-credits-rust-notices = ライセンス表示（{ $count }）

pages-credits-rust-crate = { $name } { $version } — { $license }（ライセンス表示{ $notices }）

pages-credits-rust-crate-chosen = { $name } { $version } — { $license }、{ $chosen }で使用（ライセンス表示{ $notices }）

pages-credits-rust-notice = { $number }. { $license } — { $crates }

pages-credits-rust-notice-more = { $number }. { $license } — { $first }、{ $second }ほか{ $more }件

pages-credits-rust-notice-for = 対象：{ $crates }

## Desktop's first start: the offer of other games' song folders.

pages-privacy = プライバシー
pages-privacy-intro = このお知らせは、{ -game-name }とそのオンラインサーバーがあなたについて何を保存し、どれだけの期間保存するかを説明します。最終更新：2026年11月19日。
pages-privacy-alone-heading = ひとりで遊ぶとき
pages-privacy-alone = ひとりで遊ぶとき、ゲームはすべてをあなたのデバイスに保存します。{ $platform ->
    [web] 設定、曲、プレイ履歴はブラウザーの保存領域か、コンピューター上のアプリのフォルダーに残ります。
   *[desktop] 設定、曲、プレイ履歴はコンピューター上のアプリのフォルダーに残ります。
}あなたやあなたのプレイについて、サーバーには何も送りません。リンクで共有された曲は、サーバーから読み込みます。
pages-privacy-sign-in-heading = サインイン
pages-privacy-sign-in = オンラインのルームにはTwitchでのサインインが必要です（観客席での観戦には不要です）。サインインすると、サーバーはTwitchにあなたのTwitchユーザーID、ログイン名、表示名を問い合わせます。Twitchの鍵は何も保存しません。{ $platform ->
    [web] この3つは署名付きのセッションに入り、ブラウザーがCookieとして保存します（アプリは設定フォルダーに保存します）。
   *[desktop] この3つは署名付きのセッションに入り、アプリが設定フォルダーに保存します。
}セッションの有効期間は1週間で、使っている間は更新されますが、サインインから30日を超えることはありません。サーバーはサインインした人の一覧を保存しません。サインアウトすると、セッションはデバイスから削除されます。Twitchにメールアドレスを求めることはありません。
pages-privacy-rooms-heading = ルーム
pages-privacy-rooms = ルームには、メンバーに見えている情報が保存されます。ルーム名、各メンバーの表示名とTwitchユーザーID（戻ってこられるように）、役割、ルームの曲リスト（各曲の曲名、アーティスト、パック、譜面と、誰が追加したか）、選ばれた曲、対戦の結果です。ルームは終わると削除されます。最後のメンバーの接続が切れてから2分後、1週間動きがなかったとき、ホストが抜けて引き継がれなかったとき、またはサーバーの管理者が閉じたときです。その後は、リンクでルームが閉じたことを伝えられるよう、終わったという記録（ルームのID、終わった日時と理由）だけが1週間残ります。下で説明するセッションログは別に保存され、誰の名前も含みません。ルームが開いている間、サーバーの管理者はルーム名、メンバーの表示名、人数を一覧で見られます。
pages-privacy-songs-heading = 曲
pages-privacy-songs = ルームのホストまたは共同ホストがルームの曲リストに曲を追加すると、ルームの全員が遊べるよう、ゲームがその曲を追加した人のデバイスからサーバーにアップロードし、ルームは誰が追加したかを保存します。この複製は、ルームが開いている間はリンクを知っている人なら誰でも読み込めます。複製はルームがある限り保存され、リストから削除されたとき（追加した人やホストによって、またはいっぱいのリストに空きを作るため）とルームが終わったときに削除されます。ルームが終わらずに止まってしまった場合も、ルームがまだ開いていると最後に示してから遅くとも1週間で削除されます。また、鍵を渡した一部の人は曲をリンクで共有できます。その曲は、共有するときに選んだ期限（1時間から無期限まで）まで、または削除されるまで保存されます。
pages-privacy-audience-heading = 観客席での観戦
pages-privacy-audience = ルームは、サインインしなくても観客席から観戦できます。ひとつのネットワークがすべての席を取らないよう、サーバーはあなたのネットワークのアドレスを目印に変えます。秘密の鍵で作る、毎日変わるコードで、アドレスに戻すことはできません。アドレスそのものは保存しません。目印は席が開いている間だけサーバーのメモリーにあり、サーバーの管理者に見えるのは最初の8文字だけです。サインインしている場合は、代わりにアカウントで席を数え、目印は作りません。
pages-privacy-stats-heading = 利用統計
pages-privacy-stats = オンライン対戦がうまく動いているかを確かめるため、サーバーはルームでの出来事を1日ごとの合計として数えます。たとえば次のようなものです。
pages-privacy-stats-devices = { $platform ->
    [web] 接続を開いたデバイスとブラウザーの種類（スマートフォン、タブレット、コンピューター／Chrome、Edge、Firefox、Safari、Samsung Internet、その他）。接続を開くときにブラウザーのUser-Agentから読み取り、その後は破棄します。
   *[desktop] 接続を開いたデバイスの種類（スマートフォン、タブレット、コンピューター、このアプリ）。接続を開くときにUser-Agentから読み取り、その後は破棄します。
}
pages-privacy-stats-connections = 接続がどう切れたか、どれだけ続いたか、席を離れていた時間と戻ってきたかどうか、あなたのネットワークからサーバーまでの往復時間（いずれも範囲で）。
pages-privacy-stats-loading = 曲のアップロードと読み込みにかかった時間、プレイヤーがステージに参加しなかった理由。
pages-privacy-stats-play = 遊ばれた内容：譜面の難易度の範囲とスタイル、ルール、ライフが尽きた時点、送った攻撃と踏んだショック、適応のはしごでの譜面の移動。
pages-privacy-stats-people = 1日にルームにいた人数（重複なし）。
pages-privacy-stats-kept = これらの合計には、名前、アカウント、アドレス、ルーム、曲名は含まれません。誰かを知らずに1日の人数を数えるため、サーバーは毎日変わる秘密の鍵で各アカウントからコードを作って保存し、その日を数え終えたら削除します。合計は13か月間は1日ごと、その後は1か月ごとに保存します。ボットとゲームのコンピュータープレイヤーは、人とは分けて数えます。ひとりで遊んだ分が数えられることはありません。
pages-privacy-stats-game = { $platform ->
    [web] オンラインのルームにいる間、ゲームはこれらの合計のために、ルームへの接続を開くたびに、いくつかのことをサーバーに伝えます。ブラウザーで動くゲームであること、システム（Windows、macOS、Android、iOSなど）、デバイスがスマートフォン、タブレット、コンピューターのどれか、WebGPUとWebGL2のどちらで描いているか、画面がタッチスクリーンかどうか、ゲームのバージョン、ルームに何回接続したか、今回の接続に何回の試行が必要だったか、そしてルームへの前回の接続がどう終わったか（閉じられたか途切れたか、どのコードか）、その接続がどれだけ続いたか、その間ページがバックグラウンドにあった時間、接続を保つためのメッセージに応答があったか、サーバーまでの往復時間です。どれも決まった一覧の値か範囲で、名前、アカウント、アドレス、ルーム、曲は含みません。サーバーはこれらを合計に加え、それ以外は何も保存しません。止めるには、オプション → データ →「利用統計を送る」をオフにしてください。ゲームはこれらを何も送らなくなり、上のサーバー自身による集計は続きます。観客席での観戦では、これらは何も送りません。
   *[desktop] オンラインのルームにいる間、アプリはこれらの合計のために、ルームへの接続を開くたびに、いくつかのことをサーバーに伝えます。アプリであること、システム（Windows、macOS、Linux）、Metal、Vulkan、DirectX 12、OpenGLのどれで描いているか、ゲームのバージョン、ルームに何回接続したか、今回の接続に何回の試行が必要だったか、そしてルームへの前回の接続がどう終わったか（閉じられたか途切れたか、どのコードか）、その接続がどれだけ続いたか、その間ウィンドウが隠れていた時間、サーバーまでの往復時間です。どれも決まった一覧の値か範囲で、名前、アカウント、アドレス、ルーム、曲は含みません。サーバーはこれらを合計に加え、それ以外は何も保存しません。止めるには、オプション → データ →「利用統計を送る」をオフにしてください。アプリはこれらを何も送らなくなり、上のサーバー自身による集計は続きます。観客席での観戦では、これらは何も送りません。
}
pages-privacy-sessions-heading = ルームのセッションログ
pages-privacy-sessions = ルームで起きた問題（たとえば、なぜ接続が切れたのか）を後から理解できるよう、各ルームはセッションの短いログを保存します。各接続がいつ開き、いつどのコードで閉じたか、誰かが席を離れていたときと戻ってきたかどうか、各席の役割と、遊んだか観戦したか、上の合計と同じ範囲での各席のデバイスとブラウザーの種類、曲のアップロードと読み込みにかかった時間、誰が準備できたか、誰がどんな理由でステージに参加しなかったか、プレイがどう終わったか、そして誰かが断られたこと（たとえばルームが満員だったとき）です。ログは誰の名前も含みません。各人はそのルームの中でだけ意味を持つ席番号でしかなく、名前、アカウント、アドレス、ルーム名やルームのID、曲名は含まれません。ボットとゲームのコンピュータープレイヤーはそれとわかるように記されます。「利用統計を送る」をオフにしていれば、サーバーが自分で見たことだけが残ります。ログはルームが開いている間はルームが保存し、ルームが終わると、サーバーが管理者が読めるよう14日間保存してから削除します。
pages-privacy-errors-heading = エラーレポート
pages-privacy-errors = { $platform ->
    [web] ブラウザーでゲーム自体に問題が起きたとき（クラッシュ、ゲームのコードで誰も受け止めなかった失敗、グラフィックスデバイスの喪失や不在、ゲームを始められないページ）、ゲームはサーバーに短いレポートを送ります。含まれるのは多くても、エラーの種類、表示していた画面の種類（曲選択やプレイなど。ページのアドレスは送りません）、ゲームのバージョン、システム、デバイスの種類、使っているグラフィックス、エラーのメッセージです。ゲームはメッセージから、ファイルのパス（ゲームのプログラムのどこでエラーが起きたかを除く）、ウェブやメールのアドレス、ネットワークのアドレス、引用符で囲まれた文字列、長い数字、ID、英語以外の文字を取り除き、300文字までに切り詰めます。それでも、ゲーム自身のメッセージが引用符で囲まなかった曲名の一語のような、ふつうの英単語が残ることはあります。サーバーは、利用統計と同じようにブラウザーのUser-Agentからブラウザーとデバイスの種類を読み取って破棄し、メッセージから同じものをもう一度取り除いて、それでも残っているレポートは捨てます。レポートは種類、バージョン、画面、ブラウザー、デバイス、グラフィックスごとに1日の合計として数え、異なるエラーごとにメッセージの例を一つ、最後に届いてから30日間保存します。大量に送られないよう、上で説明した毎日変わる目印で、ひとつのネットワークからのレポートを1分間だけメモリー上で数えます。ゲームが送るのは1回の訪問で5件までで、同じ訪問で同じエラーを2度送ることはありません。止めるには、オプション → データ →「エラーレポートを送る」をオフにしてください。ブラウザーがグローバル・プライバシー・コントロール（Global Privacy Control）の信号を送っていれば、この設定にかかわらず、ゲームはエラーレポートを送りません。コンピューターのアプリはエラーレポートを送りません。
   *[desktop] アプリはエラーレポートを送りません。
}
pages-privacy-logs-heading = サーバーのログ
pages-privacy-logs = サーバーはCloudflare上で動いています。Cloudflareはサーバーへの接続を扱い、受け取ったリクエストのログを最長7日間保存します。このログには、要求されたアドレス（たとえば招待リンク）や接続の技術的な情報が含まれることがあります。ログに書くこちらの記録は、ルームをIDで示し、プレイヤーの名前は書きません（サーバーの設定を変えた管理者の名前だけを書きます）。
pages-privacy-questions-heading = お問い合わせ
pages-privacy-questions = このお知らせでわかりにくい点があれば、ゲームのリポジトリでIssueを開くか、me@sugoijan.dev までご連絡ください。
pages-privacy-page = { $platform ->
    [web] ゲームのサイトでこのお知らせを読む
   *[desktop] Webでこのお知らせを読む
}

pages-offer-title = 手持ちの曲を追加しますか？

pages-offer-found = このコンピューターに、他のダンスゲームの曲フォルダーがあります。このゲームはそれらをその場所のまま読み込めます。コピーや変更は一切しません。

pages-offer-more = ほか{ $count }件

pages-offer-where = 曲フォルダーの一覧はオプションにあり、いつでも追加・削除できます。

pages-offer-decline = 追加しない

pages-offer-add = { $count ->
   *[other] 追加する
}

pages-calibration-run-visual-title = キャリブレーション 1/3：矢印のみ（無音）
pages-calibration-run-audio-title = キャリブレーション 2/3：音のみ
pages-calibration-run-combined-title = キャリブレーションの確認：音と矢印

pages-calibration-run-visual-hint = 矢印が判定ラインに重なった瞬間に押してください。自動で調整し、測定値が安定すると止まります。

pages-calibration-run-audio-hint = クリック音に合わせて、どれかの矢印キーを押してください。自動で調整し、測定値が安定すると止まります。

pages-calibration-run-combined-hint = 普段どおりにプレイしてください。自動で調整し、測定値が安定すると止まります。
