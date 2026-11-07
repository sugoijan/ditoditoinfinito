## 選曲。

## 曲の一覧。

select-pack-bundled = 収録曲

select-pack-songs = { $count }曲

select-pack-songs-found = { $count }曲中{ $found }曲

select-row-no-charts = { $style }の譜面なし

## ヘッダー。

select-title = 選曲

select-title-shared = 共有された曲

select-search-placeholder = 検索  /

select-playing-with = オプション

## 表示中の曲。

select-info-offset = オフセット { $offset } ms

select-info-line = { $artist } · { $bpm } BPM · { $length }

select-info-length = 長さ

select-info-pack = パック

## スタイルと譜面。

select-style-button = { $charts ->
    [0] { $others ->
        [0] { $style }  譜面なし  ▾
       *[other] { $style }  譜面なし · 他{ $others }スタイル  ▾
    }
   *[other] { $others ->
        [0] { $style }  { $charts }譜面  ▾
       *[other] { $style }  { $charts }譜面 · 他{ $others }スタイル  ▾
    }
}

select-play-button = プレイ

select-play-chart = プレイ { $chart }

select-autoplay-chart = オートプレイ { $chart }

select-play-no-charts = 譜面なし

select-assist-mark = ASSIST

select-no-charts = この曲には{ $style }の譜面がありません。

select-no-charts-hint = 上の点灯しているスタイルに譜面があります。Tabキーで次のスタイルに移ります。

select-chart-notes = { $notes }ノーツ

select-chart-notes-by = { $notes }ノーツ · 譜面作者：{ $credit }

select-version-without-onigiri = おにぎりなし

select-version-without-center = 中央なし

select-version-from-pump-single = Pumpシングルから

select-version-from-five-keys = 5keyから

## フッター。

select-hint-song = 曲

select-hint-chart = 譜面

select-hint-style = スタイル

select-hint-play = プレイ

select-hint-search = 検索

select-hint-start-menu = タイトルへ

select-hint-open-close = 開閉

select-hint-choose = 選択

select-hint-go-to = 決定

select-hint-manage = 管理

select-hint-plays = 履歴

select-hint-edit = 編集

select-hint-navigate = 移動

select-hint-activate = 決定

select-typing-note = 曲名、アーティスト、パック、譜面作者を検索します。かなでもローマ字でも検索できます。

select-matches = { $count ->
    [0] 条件に合う曲はありません。
   *[other] { $count }曲が見つかりました。
}

select-shared-note = 共有：{ $pack }

select-shared-note-expires = 共有：{ $pack } · { $expires }

select-share-list-note = 共有リスト：{ $count }曲

select-link-manage = 管理…

select-link-add = + 曲を追加

select-link-options = オプション

select-gesture-link = Enterキーかタップでリンクを開きます。

select-gesture-search = /キーか入力欄のタップで入力できます。

## その他のメニュー。

select-more-title = 曲の操作

select-more-search = 検索

select-more-suggestions = 検索候補…

select-more-clear-search = 検索をクリア

select-more-history = 譜面のプレイ履歴

select-more-watch-best = ベストのリプレイ

select-more-edit = 譜面を編集

select-more-manage = 曲・パックの管理

select-more-share-remove = 共有リストから外す

select-more-share-add = 共有リストに追加

select-more-share-list = { $count }曲を共有…

select-more-share-clear = 共有リストを空にする

select-more-share-song = この曲を共有…

select-more-share-pack = パックを共有…

select-more-previous-chart = 前の譜面

select-more-next-chart = 次の譜面

select-more-style = スタイル：{ $style }

select-more-add-songs = 曲を追加

select-more-options = オプション

select-suggestions-title = 検索候補

select-suggest-title = 曲名
select-suggest-artist = アーティスト
select-suggest-pack = パック

## リンクで共有。

select-share-default-pack = 共有された曲

select-share-expiry-hour = 1時間
select-share-expiry-day = 1日
select-share-expiry-week = 1週間
select-share-expiry-month = 1か月

select-share-expiry-never = 無期限

select-share-copy-link = リンクをコピー

select-share-title = 「{ $title }」を共有

select-share-title-list = { $count }曲を共有

select-share-intro = 「{ $pack }」の{ $count }曲のコピーをアップロードします。リンクを知っている人は、期限が切れるまでプレイできます。

select-share-uploading = アップロード中…

select-share-progress = { $total }曲中{ $done }曲、{ $totalmb } MB中{ $mb } MB。

select-share-done = 共有しました

select-share-done-note = このリンクを知っている人は誰でも曲をプレイできます：

select-share-failed = 共有できませんでした

## 曲の追加（インポート）。

select-import-title = 曲を追加

select-import-tab-files = フォルダー / ZIP
select-import-tab-danoni = DanOni URL

select-import-intro = { $platform ->
    [web] StepManiaのパック（.sm、.ssc、.dwi）、Dancing☆Onigiriの作品、FFRのレベル（.swf）をインポートできます。フォルダー、ZIP、ファイルの選択か、このページへのドロップで追加します。曲はこのブラウザーにだけ保存されます。
   *[desktop] 曲フォルダー、Songsフォルダー、パックのZIPを選びます。StepManiaのパック（.sm、.ssc、.dwi）、Dancing☆Onigiriの作品、FFRのレベル（.swf）に対応しています。フォルダーとZIPは元の場所から読み込まれ、オプションに一覧表示されます。
}

select-import-intro-linked = StepManiaのパック（.sm、.ssc、.dwi）、Dancing☆Onigiriの作品、FFRのレベル（.swf）をインポートできます。フォルダー、ZIP、ファイルの選択か、このページへのドロップで追加します。選んだフォルダーの音楽と背景はフォルダーから読み込まれ、それ以外はこのブラウザーにだけ保存されます。

select-import-intro-compact = { $platform ->
    [web] フォルダー、ZIP、ファイルの選択か、ここへのドロップで追加します。ファイルはこのブラウザーに保存されます。
   *[desktop] 曲フォルダーかパックのZIPを選びます。フォルダーとZIPは元の場所から読み込まれます。
}

select-import-intro-compact-linked = フォルダーかZIPを選びます。リンクしたフォルダーは元の場所から読み込まれ、ほかのファイルはこのブラウザーに保存されます。

select-import-choose-folder = フォルダーを選択

select-import-choose-files = { $platform ->
    [web] ZIPかファイルを選択
   *[desktop] ZIPを選択
}

select-import-new-song = 音楽ファイルから新しい曲を作成

select-import-option-copy = 選んだフォルダーの曲をこのブラウザーに保存

select-import-option-videos = 背景動画も保存

select-import-option-details = パック作者向けの詳細

select-import-danoni-intro = Dancing☆Onigiriの作品のURLを貼り付けてください。

select-import-danoni-import = インポート

select-import-danoni-retry = 再試行

select-import-danoni-note = ファイルをダウンロードし、ライブラリー用に変換します。

select-import-url-placeholder = https://…

select-import-origin-heading = このアドレスからの取得を許可しますか？

select-import-origin-explain = 「今回のみ許可」は今回のインポートだけを許可します。「記憶する」はこのアドレスからの今後のインポートも許可します。「拒否」はこのリクエストを中止します。

select-import-origin-once = 今回のみ許可
select-import-origin-remember = 記憶する
select-import-origin-decline = 拒否

select-import-decision-needed = 続けるには選択が必要です。

select-import-stage-progress = { $stage } · { $total }件中{ $done }件

select-import-url-invalid = https://かhttp://で始まるURLを入力してください。

select-import-needs-gesture = ブラウザーのファイル選択は、キー入力、クリック、タップで開きます。ここにファイルをドロップすることもできます。「戻る」で曲の一覧に戻ります。

select-import-cancel = インポート中止

## 追加した曲とパックの管理。

select-manage-remove-song = { $platform ->
    [web] 曲を削除
   *[desktop] 曲を隠す
}

select-manage-reread = フォルダーを読み直す

select-manage-remove-pack = { $platform ->
    [web] パックを削除
   *[desktop] パックを隠す
}

select-manage-confirm = { $platform ->
    [web] 削除
   *[desktop] 隠す
}

select-manage-keep = 残す

select-manage-unreadable-title = 読み込めなかった曲

select-manage-busy = インポート中です。削除とフォルダーの読み直しは、インポートが終わるまでできません。

select-manage-pack = { $platform ->
    [web] このブラウザーにインポートした{ $count }曲です。
   *[desktop] 曲フォルダーから直接読み込む{ $count }曲です。
}

select-manage-pack-rereadable = { $platform ->
    [web] 読み直しが可能なフォルダーから、このブラウザーにインポートした{ $count }曲です。
   *[desktop] 曲フォルダーから直接読み込む{ $count }曲です。
}

select-manage-song-pack = パック「{ $pack }」の曲です。

select-manage-confirm-pack-title = { $platform ->
    [web] 「{ $pack }」を削除しますか？
   *[desktop] 「{ $pack }」を隠しますか？
}

select-manage-confirm-pack = { $platform ->
    [web] { $count }曲がこのブラウザーから削除されます。プレイ履歴は残ります。
   *[desktop] { $count }曲が一覧から外れます。フォルダー内のファイルは削除されず、読み直すと元に戻ります。プレイ履歴は残ります。
}

select-manage-confirm-song-title = { $platform ->
    [web] 「{ $title }」を削除しますか？
   *[desktop] 「{ $title }」を隠しますか？
}

select-manage-confirm-song = { $platform ->
    [web] この曲はこのブラウザーから削除されます。プレイ履歴は残ります。
   *[desktop] この曲は一覧から外れます。フォルダー内のファイルは削除されず、フォルダーを読み直すと元に戻ります。プレイ履歴は残ります。
}

select-manage-confirm-unreadable-title = { $platform ->
    [web] 読み込めなかった保存済みの{ $count }曲を削除しますか？
   *[desktop] 読み込めなかった{ $count }曲を一覧から外しますか？
}

select-manage-confirm-unreadable = { $platform ->
    [web] 記録が読み込めなくなっています。ファイルも削除されます。プレイ履歴は残ります。
   *[desktop] 一覧から外れます。ディスク上のファイルは削除されません。
}

## 譜面のプレイ履歴。

select-plays-title = プレイ履歴 · { $chart }

select-plays-title-version = プレイ履歴 · { $chart } · { $version }

select-plays-autoplay = AUTOPLAY

select-plays-best = ★ ベスト

select-plays-watch = 再生

select-plays-results = リザルト

select-plays-remove = 削除

select-plays-remove-confirm = 本当に削除

select-plays-empty = この譜面のプレイはまだ記録されていません。

select-page-newer = ‹ 新しい履歴

select-page-older = 古い履歴 ›

select-page-range = { $count }件中{ $first }–{ $last }件

## お知らせ。

select-notices-title = お知らせ
