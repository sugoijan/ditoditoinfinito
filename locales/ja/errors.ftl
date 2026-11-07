errors-not-found = 見つかりません（{ $detail }）

errors-denied = アクセスが拒否されました（{ $detail }）

errors-full = 保存容量が足りません（{ $detail }）

errors-network = ネットワーク通信に失敗しました（{ $detail }）

errors-timeout = 時間がかかりすぎました（{ $detail }）

errors-unreadable = データを読み取れませんでした（{ $detail }）

errors-unsupported = この環境では対応していません（{ $detail }）

errors-load-no-song = この曲はライブラリーにありません

errors-load-file = 「{ $file }」を読み取れません：{ $error }

errors-load-missing = 保存された曲に「{ $file }」がありません。もう一度インポートしてください

errors-load-folder-unknown = この曲の読み込み元フォルダーを特定できません。もう一度インポートしてください

errors-load-folder-gone = この曲の読み込み元フォルダーの情報が残っていません。もう一度インポートしてください

errors-load-not-in-folder = 「{ $file }」はフォルダー「{ $folder }」にもうありません。フォルダーを読み直すか、曲をもう一度インポートしてください

errors-load-chart = 譜面「{ $file }」を読み取れません：{ $error }

errors-load-music = 曲の音声をデコードできませんでした：{ $error }

errors-load-work-gone = 作品はもう保存されていません。もう一度インポートしてください

errors-load-work = Dancing☆Onigiriの作品を読み取れません：{ $error }

errors-load-share = 共有を読み取れません：{ $error }

errors-load-audio = 音声の再生を開始できませんでした：{ $error }

errors-video-not-included = このビルドには動画デコーダーが含まれていません

errors-video-not-loaded = 動画デコーダーを読み込めませんでした：{ $error }

errors-video-crashed = 動画デコーダーが異常終了しました（{ $detail }）

errors-video-unknown-format = デコーダーが対応していない動画です

errors-video-broken = { $count ->
   *[other] 動画をデコードできません（壊れたフレームが{ $count }枚続きました）
}

errors-video-unseekable = シーク後に動画を読み直せませんでした

errors-video-too-large = フレームの大きさ（{ $width }×{ $height }）がGPUの上限{ $limit } pxを超えています

errors-video-bad-frame = 壊れたフレームがありました

errors-share-gone = リンクが存在しないか、期限が切れています

errors-share-no-token = 先にオプションの「共有」でアップロード用トークンを設定してください

errors-share-nothing = 共有するものがありません

errors-share-token-refused = アップロード用トークンが拒否されました

errors-share-too-large = 大きすぎて共有できません（共有ごとに2 GB、ファイルごとに512 MB、200曲まで）

errors-share-status = サーバーがHTTP { $status }を返しました

errors-share-status-body = サーバーがHTTP { $status }を返しました：{ $body }

errors-share-answer = サーバーの応答を読み取れませんでした（{ $detail }）

errors-share-no-chart = 「{ $song }」の譜面を読み込めませんでした

errors-share-in = { $name }：{ $error }

errors-share-link-gone = このリンクは存在しないか、期限が切れています。

errors-share-not-loaded = 共有された曲を読み込めませんでした：{ $error }。

errors-share-not-shared = 曲を共有できませんでした：{ $error }。

errors-shared-page-title = 共有された曲

errors-shared-page-hint = 共有リンクでは、共有元の曲を、共有した人が指定した期間だけプレイできます。共有した人に新しいリンクを依頼してください。

errors-shared-page-home = タイトルへ

errors-history-too-long = { $mb } MBより長い行です

errors-history-not-utf8 = UTF-8のテキストではありません

errors-history-not-a-play = プレイの記録ではありません：{ $detail }

errors-history-not-a-play-id = IDがプレイのものではありません

errors-history-record-not-json = 記録がJSONではありません

errors-history-record-no-version = 記録にバージョンがありません

errors-history-record-version-zero = 記録のバージョンが0です

errors-history-record-unreadable = 記録を読み取れません：{ $detail }

errors-history-record-out-of-range = 記録の時刻かオフセットが範囲外です

errors-history-record-no-song = 記録に曲が指定されていません

errors-history-log-out-of-range = 入力記録の時刻が範囲外です

errors-history-newer-record = 新しいバージョンのゲームで保存されています（記録形式 { $version }）

errors-history-newer-judge = 新しいバージョンのゲームで判定されています（判定処理のバージョン{ $version }）

errors-history-newer-log = 入力記録が新しい形式です（{ $version }）

errors-history-newer-timing = タイミングが新しい形式です（{ $version }）

errors-history-log-unreadable = 入力記録を読み取れません：{ $detail }

errors-history-timing-unreadable = タイミングを読み取れません：{ $detail }
