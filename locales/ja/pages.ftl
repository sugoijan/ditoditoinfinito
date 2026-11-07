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
