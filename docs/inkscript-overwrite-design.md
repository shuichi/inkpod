# InkScript の上書き保存

同一directoryのtemporaryを完成・flush・closeした後、一回のatomic renameで公開し、
保存直前のfingerprint照合で外部変更を検出する方式である。TxFには依存しない。
現在の状態と検証結果は [compatibility](compatibility.md) を参照する。

## 適用範囲

この方式はM4のCore-only入出力に対する承認済み契約であり、M5／製品cutoverは別工程である。
[INKSCRIPT 7.9](../INKSCRIPT.md#79-output)のoverwrite confirmation、closed native input自身への限定、
open-session拒否を維持し、最終fingerprint検査とsourceの通常の書込み排除を行う。
検査後に別processがdestination名を変更・置換する競合まで取りこぼさない保証はしない。

[INKSCRIPT 8.1](../INKSCRIPT.md#81-static-compileplantaskconfirmation)はtemporaryのrelative再openを基本とし、
親authorityとtemporary identityを照合するabsolute再openも許可する。本方式は既存のrelative openを利用する。

## 保存手順

1. 既存の共有I/O managerでdestination pathとfile identityのapplication内lockを取る。
   plan、confirmation、open-session registry、sourceとdestinationのauthorityを再検証する。
2. 検証したdestination parent内にtemporaryをexclusive createする。衝突retryはboundedとし、
   元fileへ直接write／truncateするfallbackは設けない。
3. 全bytesを書き、write、flush、syncの成功を確認してwriterをcloseする。その後temporaryの
   非writing control handleを取得し、作成時のidentityとstampを照合する。親authorityも保持する。
4. overwriteのsourceをreadで開き、FILE_SHARE_READ | FILE_SHARE_DELETEを指定する。
   このhandleで完全なdigestとidentity／stampをplanned fingerprintと照合する。
   WRITE共有を許可せず、通常のfile openによる同一objectへのwrite／truncateを置換まで拒否する。
   overwrite以外のsource guardは従来の共有modeを維持する。
5. cancel、authority、destination pathが指すidentity、confirmationとopen-session generationを
   置換直前にも検査する。不一致はstaleとして停止し、最新fileへの読み替えや自動上書きretryをしない。
6. temporary control handleとparent handleを保持したまま、既存の
   NtSetInformationFile(FileRenameInformationEx, REPLACE_IF_EXISTS | POSIX_SEMANTICS)で置換する。
   非上書きは従来どおりatomic create-if-absentとする。copy→deleteや元fileの先行削除へfallbackしない。
7. rename成功を一件の公開時点とする。cacheを無効化し、遅いcancelで成功を取り消さない。
   失敗・公開前cancelでは自分が作ったidentityのtemporaryだけをcleanupする。
   他itemの成功済みoutput、source/live sessionのsavepointとpath authorityは変更しない。

これらは既存のI/O managerとprivate Windows backendで扱う。新規crate、TxF、kernel driver、
第二のI/O engineは追加しない。

## 保証の境界

| 条件 | 保存時の挙動 |
| --- | --- |
| encode／write／flush失敗、公開前cancel | 未完成bytesをdestinationへ公開しない。自処理は元fileを変更しない |
| 外部変更を最終検査までに観測した | staleとして停止し、出力をinstallしない |
| guard中の通常の外部write／truncate | 開いているsource objectへの書込みopenを拒否する |
| 最終検査後、rename前の外部rename／delete／別objectへの置換 | 完全な検出を保証しない。後から成功した置換がdestination名に残る可能性がある |
| 同じapplicationの競合job | 共有managerのpath／identity lockで直列化する |
| atomic renameを提供しない環境 | 明示エラーとし、非atomicな上書きへ切り替えない |
| crash／電源断 | 完成したtemporaryをsyncしてから公開するが、全filesystem／deviceの電源断耐性を一律には保証しない |

例えば、最終検査の直後に別アプリが同じ名前へ新しいfileを保存し、その直後にinkpodのrenameが成功すると、
別アプリの新しいfileを置き換える可能性がある。これは内容を途中まで書いて壊す問題とは別の競合であり、
公開後の再検査で巻き戻そうとしても第三の変更を壊し得るため、無条件rollbackで解決しない。

FILE_SHARE_DELETEは自分の置換を可能にするが、他processの名前変更も許す。最後にもう一度checkしても
checkとrenameを一つの条件付き操作にはできない。厳密なno-lost-updateは保証しない。

## APIの選択理由

一時fileを完成してから置換する構成は、[Qt QSaveFile](https://doc.qt.io/qt-6/qsavefile.html) や
[GIO File.replace](https://docs.gtk.org/gio/method.File.replace.html) が採る保存方式と共通する。
GIOのetag照合も、読込後の変更検出を別の条件として扱う。これらのlibraryには依存しない。

Microsoftも [TxFの代替](https://learn.microsoft.com/en-us/windows/win32/fileio/deprecation-of-txf) として
新しいfileを用意して置換する文書保存を案内している。POSIX renameの既存handle保持と新規openへの
切替は [FILE_RENAME_INFORMATION](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/ns-ntifs-_file_rename_information)
に記載されている。

既存のhandle-relative renameを維持する理由は、temporaryとparentの保護を保ったまま置換でき、
M4の変更をsourceの共有modeと保証の明文化へ絞れるためである。
[ReplaceFileW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-replacefilew) は
DACLやnamed stream等を継承する用途には利点があるが、temporaryを再openし、特定の失敗では元の名前が
残らない状態も文書化されている。本方式では追加のbackup／復旧経路を要するReplaceFileWを使用しない。
通常のrenameが全metadataを継承するとは主張せず、保存内容の保証とmetadata継承を混同しない。

## 検証する契約

- overwrite経路だけDELETE共有を許可し、sourceのWRITE拒否、cancel、上書き成功を検証する。
  それ以外のsource guardは従来の共有modeを維持する。
- 最終検査前に外部rename／replacementを注入するとstaleとして拒否し、外部bytesを保つ。
  最終検査後のname raceは保証外の境界として明記し、確率的stressの成功を完全排除の証拠にしない。
- temporary差替え、parent／authority失効、同一identityの変更、競合writer、write／flush／install失敗、
  cancellation、cleanup、cache、Coreからのsave／reopenを検証する。
- [検証手順](verification.md)の対象public tests、format、workspace lint／testと共有境界のWindows検証を行う。
  他filesystemやx64／ARM64は実行した構成だけを記録し、未対応と未検証を区別する。

この保存方式はfile grammar、catalog、canonical replay、native bytes、C ABIを変更しない。
版はexact-currentを維持し、承認済みworkload／性能基準は変更しない。
