## 曲のインポートの報告（曲を追加）。

import-step-reading-files = ファイルを読み込み中…

import-step-reading-zip = { $path }を読み込み中…

import-step-shared-images = 共有背景画像{ $count }枚を保存中…

import-step-shared-videos = 共有背景動画{ $count }本を保存中…

import-step-song = インポート中 { $index }/{ $total }：{ $name }

import-step-work = 「{ $label }」をインポート中…

import-step-level = レベルを読み込み中 { $index }/{ $total }：{ $title }

import-step-skin = ノートスキン「{ $name }」を保存中…

import-step-skin-lua = ノートスキン「{ $name }」のLuaを実行中…

import-step-announcer = アナウンサー「{ $name }」を保存中…

import-step-stills = 曲の動画から画像を作成中… { $total }曲中{ $done }曲

## インポート後の要約（「 · 」で区切ったタグ）。

import-summary-songs = { $count }曲をインポート

import-summary-noteskins = { $count }件のノートスキンを保存（オプションの「矢印」で選べます）

import-summary-announcers = { $count }件のアナウンサーを保存（オプションの「サウンド」で選べます）

import-summary-shared-images = { $count }枚の共有背景画像を保存

import-summary-shared-videos = { $count }本の共有背景動画を保持

import-summary-shared-skipped = { $count }個を除外（共有フォルダー内のほかのファイル）

import-summary-videos = { $count }本の背景動画を再生

import-summary-videos-left-out = { $count }本の背景動画を除外（「背景動画も保存」がオフ）

import-summary-removed = { $count }曲を削除（フォルダーに存在しない曲）

import-summary-warnings = { $count }件の警告

import-summary-skipped = { $count }件をスキップ

import-summary-details = { $count }件の詳細

import-nothing-found = 選んだものに.sm、.ssc、.dwiファイル、Dancing☆Onigiriの作品、FFRのレベル、ノートスキン、アナウンサーが見つかりませんでした。

import-no-simfiles = 選んだものに譜面ファイルが見つかりませんでした。

## 報告の見出し。

import-section-noteskins = ノートスキン

import-section-announcers = アナウンサー

import-section-skipped = 未インポート

import-section-problems = インポート済み（問題あり）

import-section-details = パック作者向けの詳細（プレイには影響しません）

## 報告の行：対象と理由。

import-line = { $what }：{ $why }

## ファイルとアーカイブ。

import-file-failed = { $file }：{ $error }

import-file-missing = { $file }がありません

import-truncated = { $file }が途中で切れています（ZIPが不完全です）

import-encrypted = 暗号化されているため読み込みません

import-zip-not-a-zip = ZIPファイルではありません（セントラルディレクトリの終端レコードがありません）

import-zip-corrupt = ZIPファイルが壊れています：{ $detail }

import-zip-multi-disk = 複数のファイルに分割されたZIPには対応していません

import-zip-encrypted = 「{ $name }」は暗号化されています。暗号化されたZIPの項目には対応していません

import-zip-method = 「{ $name }」は圧縮方式{ $method }を使っています。対応しているのはstored、deflate、Deflate64、bzip2、LZMA、Zstandardだけです

import-zip-size = 「{ $name }」：{ $expected }バイトのはずが{ $actual }バイトです

import-zip-crc = 「{ $name }」：CRC-32が一致しません（期待値 { $expected }、実際 { $actual }）

import-zip-inflate = deflateのデータが不正です：{ $detail }

import-zip-decode = 「{ $name }」：{ $method }のデータが不正です：{ $detail }

## StepManiaの譜面ファイルと曲のファイル。

import-no-charts = 譜面ファイルに譜面がありません

import-no-playable-charts = このゲームに対応したレイアウトの譜面がありません（検出したレイアウト：{ $layouts }）

import-no-music = 譜面ファイルと同じフォルダーに音声ファイルがありません

import-same-folder = { $first }と同じパック名・フォルダー名です

import-music-without-simfile = 音楽（{ $music }）はありますが、.sm、.ssc、.dwiの譜面ファイルがありません

import-layouts-left-out = このゲームで遊べないレイアウトの譜面は除外しました：{ $layouts }

import-missing = { $tag }が指定する「{ $file }」がありません

import-missing-preview = { $tag }が指定する「{ $file }」がありません。代わりに曲の一部をプレビューとして再生します

import-missing-chart-music = { $tag }が指定する「{ $file }」がありません。代わりに曲の音楽が流れます

import-missing-banner = { $tag }が指定する「{ $file }」がありません。この曲にはバナーがありません

import-missing-background = { $tag }が指定する「{ $file }」がありません。この曲には背景がありません

import-missing-replaced = { $tag }が指定する「{ $file }」がありません。代わりに「{ $used }」を使います

import-tag-chart-music = #MUSIC（譜面{ $chart }）

import-too-many-bg-images = 背景切り替えの画像が{ $count }枚あります。使うのは最初の{ $kept }枚だけです

import-preview-video-not-stored = プレビュー動画{ $file }は保存しませんでした（「背景動画も保存」がオフ）

import-preview-video-unplayable = #PREVIEWVIDが指す{ $file }は、このゲームで再生できない形式です

import-videos-unplayable = このゲームで再生できない形式の背景動画です（代わりに曲の背景を表示します）：{ $videos }

import-video-item = { $file }（{ $format }）

import-video-format = { $container }の{ $codec }

import-video-format-unknown = 不明な形式

import-video-mpeg-ps = MPEGプログラムストリーム
import-video-mpeg-ts = MPEGトランスポートストリーム
import-video-mpeg-video = MPEGの生ストリーム

import-videos-not-stored = 保存しなかった背景動画（「背景動画も保存」がオフ）：{ $files }

import-too-many-script-files = スクリプト背景のファイルが{ $count }個あります。使うのは最初の{ $kept }個だけです

import-shared-animations = 背景切り替えが共有フォルダーのアニメーションを使っていますが、再生されません（代わりに曲の背景を表示します）：{ $names }

import-missing-animations = 背景切り替えが指定するスクリプトアニメーションがありません（この種類のアニメーションは再生対象外です）：{ $names }

import-missing-backgrounds = 背景切り替えが指す画像や動画が見つかりません（代わりに曲の背景か、RandomMoviesフォルダーのランダムな動画を表示します）：{ $names }

## Dancing☆Onigiriの作品。

import-divided-dos = 譜面データ（dos）が複数のファイルに分かれています。これにはまだ対応していません

import-dos-missing = dosファイル{ $file }がありません

import-dos-not-read = { $file }を読み込めませんでした

import-dos-script = 「{ $file }」はスクリプトでdosを生成します。インポート時にはこのスクリプトを実行しません

import-work-music-missing = 音楽{ $url }が見つかりません

import-work-without-music = 作品が音楽を指定していません

import-music-script-empty = 音楽のスクリプトに音声が入っていません

import-layer-images-missing = 背景・マスク画像が見つかりません（またはSVGなど描画できない形式です）：{ $names }{ $more ->
    [0] {""}
   *[other] 、ほか{ $more }枚
}

import-too-many-layer-images = 背景・マスク画像が{ $count }枚あります。使うのは最初の{ $kept }枚だけです

import-capture-without-layers = キャプチャーには背景・マスク画像が含まれていません

import-same-file = { $first }と同じパック名・ファイル名です

import-music-not-captured = 作品の{ $count }曲は音楽がキャプチャーされていません：{ $titles }

## FFRのレベル。

import-level-without-mp3 = レベルにMP3の音楽がありません

import-level-without-music = beatBoxと同じフォルダーに音声ファイルがありません

import-swf-not-swf = SWFファイルではありません

import-swf-inflate = SWFを展開できません：{ $detail }

import-swf-lzma = LZMA圧縮のSWF（ZWS）は読み込みません

import-swf-cut-short = SWFが途中で切れています

import-swf-string = SWF内の文字列に終端がありません

import-swf-music-too-long = 音楽が長すぎます

## インポートしたノートスキン。

import-skins-other-games = ノートスキン{ $count }件：ほかのゲーム用です（{ $games }）。danceとpump用ではありません

import-too-many-skins = ノートスキン{ $count }件：インポートしたのは最初の{ $kept }件だけです

import-same-skin-name = 今回のインポートの別のノートスキンと同じ名前です

import-too-many-skin-files = 画像とスクリプトが{ $count }個あります。保存したのは最初の{ $kept }個だけです

import-skin-file-too-large = { $file }は大きすぎるため除外しました

import-skin-texts-too-large = テキストが合計{ $megabytes } MBを超えたため、残りは除外しました

## ノートスキンの描画の内容。

import-skin-common = StepManiaの共通ファイル（common）です。ほかのノートスキンが不足する要素の代わりに使います

import-skin-pump = pump用スキン（Pumpシングルで確認）：{ $summary }

import-skin-lua = Luaを実行：{ $total }要素中{ $own }要素がスキン独自です

import-skin-lua-partly = Luaを実行：{ $total }要素中{ $own }要素がスキン独自、{ $builtin }要素は内蔵の描画です

import-skin-lua-partly-why = Luaを実行：{ $total }要素中{ $own }要素がスキン独自、{ $builtin }要素は内蔵の描画です（{ $why }）

import-skin-no-lanes = このレイアウトのレーンは描画できません

import-skin-lua-stopped = Luaが停止しました（{ $error }）。Luaなしで描画します：{ $files }

import-skin-legacy = StepMania 3.9・OpenITG用：{ $total }要素中{ $own }要素がスキン独自です

import-skin-legacy-partly = StepMania 3.9・OpenITG用：{ $total }要素中{ $own }要素がスキン独自、{ $builtin }要素は内蔵の描画です

import-skin-legacy-partly-why = StepMania 3.9・OpenITG用：{ $total }要素中{ $own }要素がスキン独自、{ $builtin }要素は内蔵の描画です（{ $why }）

import-skin-files = { $total }要素中{ $own }要素がスキン独自の画像とモデル{ $lua ->
    [0] {""}
   *[other] 、{ $lua }要素はLua（内蔵の描画）
}{ $builtin ->
    [0] {""}
   *[other] 、{ $builtin }要素はStepMania自体のスキンのもの（内蔵の描画）
}{ $other ->
    [0] {""}
   *[other] 、{ $other }要素は描画できません
}

## インポートしたアナウンサー。

import-announcer-summary = セリフ{ $lines }件（プレイ中{ $play }件、リザルト{ $results }件、選曲{ $select }件）、音声ファイル{ $sounds }個、{ NUMBER($megabytes, minimumFractionDigits: 1) } MB

import-too-many-announcers = アナウンサー{ $count }件：インポートしたのは最初の{ $kept }件だけです

import-announcer-without-lines = ゲームで使うセリフが1つもないアナウンサーです

import-same-announcer-name = 今回のインポートの別のアナウンサーと同じ名前です

import-announcer-sounds-left-out = 音声ファイル{ $count }個を除外しました（上限は{ $files }個または{ $megabytes } MB）

import-announcer-old-folders = 今は話されないセリフのフォルダー{ $count }個：{ $names }{ $more ->
    [0] {""}
   *[other] 、ほか{ $more }個
}

import-announcer-unknown-folders = どのセリフにも使われないフォルダー{ $count }個：{ $names }{ $more ->
    [0] {""}
   *[other] 、ほか{ $more }個
}

import-announcer-empty-folders = 音声のないフォルダー{ $count }個：{ $names }{ $more ->
    [0] {""}
   *[other] 、ほか{ $more }個
}

import-announcer-nested-copies = セリフのフォルダー内のコピー{ $count }個を無視しました：{ $names }{ $more ->
    [0] {""}
   *[other] 、ほか{ $more }個
}

import-announcer-other-files = ほかのファイル{ $count }個を無視しました（音声でないか、セリフのフォルダーにないもの）

## インポートしたものの保存。

import-shared-videos-failed = 共有背景動画：{ $error }

import-pack-banner-failed = パックのバナー：{ $error }

import-pack-banners-failed = パックのバナー：{ $error }

import-removing-gone-failed = フォルダーからなくなった曲の削除：{ $error }

import-tidying-failed = 保存済みの作品とフォルダーの整理：{ $error }

import-still-failed = 背景動画「{ $movie }」をデコードできず、曲の画像を作れませんでした（{ $reason }）。代わりに背景画像を表示します

import-still-later = 今回は背景動画から曲の画像を作れませんでした（{ $reason }）。初めてプレイするときに作ります
