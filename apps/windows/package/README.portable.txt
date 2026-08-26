inkpod ポータブル版
====================

inkpod は Windows 11 向けのアニメーション彩色アプリケーションです。
インストールせずに、この ZIP を任意のフォルダーへ展開して inkpod.exe を実行できます。

この配布物の inkpod.exe には、Rust Core と Win32 フロントエンドが使用する
Microsoft Visual C/C++ ランタイムが静的リンクされています。Visual C++
再頒布可能パッケージや、同梱されていない VC Runtime DLL は必要ありません。
Direct2D、Direct3D、Win32 など、Windows 11 のシステムコンポーネントは使用します。

ポータブル版は .inkpod ファイルの関連付けを登録しません。必要な場合は Windows の
「プログラムから開く」で inkpod.exe を選択してください。ワークスペース設定、最近使った
ファイル、自動保存、復元データ、オフラインヘルプのキャッシュは、インストール版と同様に
%LOCALAPPDATA%\inkpod 以下へ保存します。通常設定に HKCU は使用しません。

ライセンスは LICENSE.txt、使用する第三者コンポーネントの通知は
ThirdPartyNotices.txt を参照してください。対応するソースコードは次で公開しています。

https://github.com/shuichi/inkpod
