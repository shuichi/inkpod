# InkScript 言語仕様・実装計画

## 1. 文書の位置付け

この文書は、廃止予定の `.inkbatch` を置き換える UTF-8 テキスト形式
`.inkscript` の言語仕様と実装計画を定める。InkScript は、既存 Batch の
`Input -> Operations -> Output` を包含し、`.inkpod` の journal に保存される
journal-replayable な canonical procedure と等価な文書変更を、別文書へ安全に
再束縛して実行できることを目的とする。

機能要件の正本は [SPEC.md](SPEC.md)、作業規律の正本は [AGENTS.md](AGENTS.md) とする。
本書の現行 language/runtime 契約と、未解除の product 公開・cutover gate は規範である。
競合はユーザーの最新指示、`AGENTS.md`、`SPEC.md`、この文書の順で解決する。

16–18 節の一 milestone ごとの停止・利用者確認・再開 prompt は、ユーザーが当該 milestone
workflow の再開を明示した場合だけ適用する。本書を参照・レビュー・更新すること自体は、
再開指示、利用者確認、承認を意味しない。M15（旧 M34）の cutover、性能基準、版数変更の承認条件と
未解除の公開 gate は、この適用範囲の区別によって解除されない。現在の要件別状態・既知差分・
代表検証は [docs/compatibility.md](docs/compatibility.md) を正本とする。17 節は今後の作業だけを
M1 から採番する。完了済み工程・旧版への更新経緯は Git 履歴を参照する。外部文書の旧 gate 名と
新番号の対応、および registry owner ID を変更しない規則は 16.5 節に置く。

現行の machine-readable contract は [registry schema v2](schemas/inkscript/registry-schema-v2.json)、
[language v3](schemas/inkscript/language-v3.json)、[catalog v8](schemas/inkscript/catalog-v8.json)、
[owner manifest v8](schemas/inkscript/owner-manifest-v8.json) である。language は command 非依存の
type、section、selector、assert、asset の exact field、型、default、上限を固定し、catalog は
75 command の閉じた集合を定義する。退役 primitive ID は tombstone として再利用しない。
production Rust compile／bind／staged-run API と、実装済みの source／export／execution C ABI、
Windows private authority／engine route を再利用する。これらの存在は `.inkscript` の product
file filter、clipboard、Batch pane への接続を意味しない。公開境界は 14–17 節に従う。M4 の input profile／codec／staged output／画像 preview は
Core-only の実行経路であり、既存 ABI の native private 経路への接続拡張は M5 で扱う。
[command reference](docs/inkscript-command-reference.md) は registry からの生成物であり、手編集しない。

本文中の「必須」「禁止」「拒否」は規範要件である。「推奨」は、同等の安全性、
決定性、保守性を示せる場合に限って置換できる設計判断である。

現在のexact-current値は次のとおりとする。

| 項目                                |                                       現在値 |
| ----------------------------------- | -------------------------------------------: |
| InkScript file format version       |                                            3 |
| InkScript procedure catalog version | 8（四処理の一 command 追加、75 command） |
| required replay epoch               | 29 |
| native output                       |                      exact-current `.inkpod` |
| native top-level format             | 34 |
| C ABI                               | 34 |

フォーマットフリーズ前のため、reader、writer、clipboard fragment は常に
exact-current version だけを受理する。grammar、serialized field、selector の
意味、command signature、実行結果を変える変更では、影響する最上位 version を
同じ変更で更新し、旧 version の migration reader、互換 writer、互換 shim を
残さない。

| 変更                                                                      | 必須version更新                                                                   |
| ------------------------------------------------------------------------- | --------------------------------------------------------------------------------- |
| lexer、grammar、section、literal、asset表現                               | file format version                                                               |
| command/assert/selectorの追加・削除・名前・field・型・result・binding意味 | procedure catalog version。serialized syntaxも変わる場合はfile versionも更新      |
| 同じtyped invocationから得るcanonical state/pixel結果                     | replay epochと`.inkpod` top-level version。必要に応じてcatalog/file versionも更新 |

catalog versionは「そのbuildで実装済みのcommand集合」ではなく、批准済みの完全なclosed command
contractを識別する。実装coverageは非永続の内部状態であり、file、clipboard、公開ABIへserializeしない。
現行は`catalog-v8.json`だけを受理し、過去のcatalogをin-place変更して現行契約へ読み替えない。
新entryやsignature変更ではcatalog versionを更新し、旧version拒否test、example、registry、生成referenceを同時更新する。

### 1.1 再開時の適用範囲

4–13 節は現在の file v3／catalog v8 の language/runtime 契約を記す。clipboard と編集 UI の記述は
未接続の受入契約であり、実装済みの主張ではない。現行 Batch の製品挙動は
[SPEC 19 節](SPEC.md#19-バッチ処理)を正本とし、次表の Core-only 契約を維持し、M5 の ABI／Windows 統合と後続の受入を経て UI を接続する。
M1 の承認は input profile／output envelope の実装契約であり、製品 cutover の承認を兼ねない。

| 対象 | 現行 InkScript の境界 | 再開後に満たす契約 |
| --- | --- | --- |
| Batch program | catalog v8 の `apply_batch_operations` と展開済み Commit の fragment export | 四種類の処理と全 target を一 canonical invocation／一 transaction／一 Undo で実行し、M9 の製品 parity へ接続する |
| 入力・出力 | file v3 の二 profile、共通codec、native naming／folder／active／new_tabs の Core-only plan/run | M5 で ABI／Windows の ownership と発行時 target へ統合し、M9 で製品 parity を確認する |
| preview | authority preview、staged dry-run、temporary copy から作る画像 contact sheet を別結果型とする Core-only API | M5 以降で元 target を保持する preview tab publication へ接続する |
| 編集 UI | lossless source と typed model の基盤。製品 editor は未接続 | 現行の固定 Input／Output、四種類の処理、set 保存、専用 Batch tab、三つの実行 button を出発点にする |
| I/O ownership | 共有 Rust I/O manager の Core-only adapter と private platform backend。既存 Windows native adapter は別経路 | M5 で Windows engine を共有 manager へ移管し、authority／atomic install の強度を保つ |

標準 layer は MainLine と Color を各一枚持ち、追加 plane は Raster とする。保存選択 mask は
document-owned collection、fill protection は selection と別の document state である。
vector／annotation／adjustment／selection layer、vanishing point、Cut 管理・指示画像 export は
再導入しない。線補正と現行の選択・背景判定は既存 canonical executor を使用する。
Batch の四種類への制限を理由に、存続する一般 InkScript command を削除しない。

## 2. 目的と非目的

### 2.1 目的

- 既存 Batch の入力選択、順序付き処理、出力、dry-run、preview、progress、
  cancellation、failure policy、atomic output を維持する。
- すべての journal-replayable な production document mutation を、型付きの
  `invoke` statement として表現可能にする。
- Batch 画面で作った処理と、Inkpod ファイルの可視化画面で選んだ Commit を、
  同じ InkScript fragment としてコピー＆ペーストできるようにする。
- 上級者がテキストエディタで安全に記述、レビュー、差分比較できるようにする。
- 同じ script version、procedure catalog、replay epoch、入力、parameter、asset、
  selector 解決結果から同じ canonical Core 結果を得る。
- 文書固有の stable ID を、明示的な selector または先行 step の結果へ再束縛する。
- 解析、検証、dry-run、失敗、cancel で入力文書を変更せず、未完了の各input itemの
  destinationへ部分出力を残さない。job全体はatomicではなく、既にatomic install済みの
  先行itemを後続itemの失敗やcancelでrollbackしない。

履歴断片の等価性は二種類に分ける。

- **exact-source replay**: 元Commit列の直前stateを基準に、最終state digest、ID
  high-watermark、typed result role、canonical procedure列が一致する。
- **rebound execution**: semantic selectorまたは先行resultへ明示再束縛した別文書で、
  解決済みroleに対して決定的に同じtyped invocationを実行する。sourceのUUID、raw ID、
  state digestとの一致は保証しない。

### 2.2 非目的

- `.inkpod` container、Genesis、完全な Undo/Redo branch graph の代替にはしない。
- `HistoryMove`、`BranchCut`、history cursor、inactive branch topology を script として
  実行しない。
- zoom、pan、active tool、dialog、file picker、window layout、renderer command など、
  view、session、OS、UI 固有操作を script command にしない。
- shell、任意 process、network、clock、locale、environment variable、registry、
  無制限 loop、recursion、動的 code loading を提供しない。
- 現行版では`include`、module、別script importを提供しない。完全file一件だけでprogram構造を
  決定し、assetの`data_file`だけを明示的な外部byte依存として許可する。
- Rust enum の `Debug` 表示や可視化画面の要約文字列を executable syntax として
  再利用しない。
- source document の `ProcedureId`、`StateId`、`JournalEventId` を別文書へ移植しない。
- `strict_source_only` procedureをpreconditionなしでportableに見せたり、別文書へrebindしない。

「任意の Inkpod 処理」とは、journal-replayable として primitive catalog に登録された
文書変更を意味する。query、preview の途中状態、export、save、open、UI command は含まない。

## 3. 基本アーキテクチャ

```text
UTF-8 .inkscript / clipboard fragment
    -> bounded lexer/parser
    -> lossless CST + source diagnostics
    -> semantic AST + exact-current static validation
    -> parameters の確定
    -> path intent preview（この時点では外部pathをopenしない）
    -> frontend authority
    -> cancellable PlanTask
    -> asset ingestion、input snapshot、output path の immutable preview
    -> plan/input/output/authorityへ束縛したconfirmation token
    -> input ごとの selector binding
    -> typed ScriptExecutionPlan
    -> preview順に既存 canonical primitive executor
    -> 通常の Commit 列を持つ staged Core
    -> 選択codecのencode／active・new-tabのstaged result
    -> input ごとのatomic install／owner-thread publication
```

InkScript は新しい画像処理 engine を持たない。各 `invoke` は既存の typed request /
`CanonicalInvocation` へ正規化され、live commit、Undo/Redo、native replay と同じ primitive
executor を使用する。C++ は parser、selector、画像処理、journal replay を実装せず、
file dialog、path authority、clipboard、UI、thread/job 接続だけを担当する。

InkScript には二つの層がある。

1. **orchestration 層**: inputs、parameters、bindings、output、execution policy
2. **document program 層**: ordered `assert` / `step` と typed canonical invocation

切替後の Batch は、document program と command catalog を利用する UI projection とする。
現行 Batch v5 の処理列を一 canonical transaction にする境界は保持し、既存 primitive executor を
再利用する。共通codec・I/O manager による入力／出力／画像 preview は M4 の Core-only scope とし、
ABI／Windows engine は M5、製品切替は M15 で扱う。

sourceは編集用のlossless CSTと実行用のsemantic ASTを分離する。外部pathを読む前までの
static compile、authority取得後の`PlanTask`、確認後の`RunTask`は別lifecycleとする。
現行版のitem実行とinstallはimmutable preview順の逐次実行に固定し、Core single-writer、
`failure = stop`、`wait_ms`、report順を一意にする。

## 4. ソースファイル

### 4.1 encoding と改行

- extension は `.inkscript` とする。
- encoding は妥当な UTF-8 とする。先頭の UTF-8 BOM は読み取り時だけ許容する。
- canonical emitterとgenerated writerはBOMなしUTF-8、LF改行、末尾改行一つを出力する。
  lossless writerは元sourceのBOMと改行byte列を保持する。
- NUL、不正 UTF-8、孤立 surrogate に相当する escape、不正な Unicode scalar を拒否する。
- identifier と keyword は ASCII、小文字、case-sensitive とする。
- string の Unicode scalar 列は保存し、NFC/NFD へ暗黙変換しない。
- source の通常 load/run は元テキストを書き換えない。UI から明示保存した場合だけ、局所編集を
  反映したlossless writerの結果をatomic replaceする。canonical emitterによる全体整形は、別の
  明示操作として扱う。
- text file全体のstored checksumは持たない。人が編集したsourceそのものを入力とし、
  asset payloadだけはcontent-addressed digestで検証する。

### 4.2 header と必須 section

完全な file は必ず次の header から始める。

```inkscript
inkscript 3;
```

header 後の section 順序は parser が許容するが、同名 section の重複を拒否する。
canonical formatter は次の順序で出力する。

1. `requires`
2. `meta`
3. `inputs`
4. `parameters`
5. `bindings`
6. `program`
7. `output`
8. `execution`
9. `assets`

`requires`、`inputs`、`program`、`output`、`execution` は完全な file で必須とする。
その他は省略できる。

### 4.3 comment

`//` から行末までを comment とする。string と Base64 literal の内部では comment を
開始しない。block comment と nested comment は現行版では提供しない。
comment は実行意味を持たない。

parser はsource textを二種類の表現へ分ける。

1. **lossless CST**: BOM、改行形式、token spelling、comment、空白、error node、UTF-8 byte
   rangeを保持する編集用表現
2. **semantic AST**: triviaとspelling差を除去し、compileと実行に使用する表現

parse errorがあってもlossless CSTと診断は返してよいが、semantic AST、
`ScriptExecutionPlan`、実行可能fragmentを公開してはならない。未編集のlossless CSTを
再出力したbyte列は入力byte列と完全一致しなければならない。

通常の保存はlossless CSTから出力し、ユーザーが記述したcomment、空行、field配置を可能な
限り保持する。GUIによる局所編集は変更対象nodeと必要な区切りだけを書き換え、無関係なsource
rangeを再formatしない。局所的に安全な書換えができない場合はsource全体を暗黙canonical化せず、
明示的なformat/rewrite確認を要求する。comment、trivia、元のliteral spellingは実行意味、
semantic digest、dependency closure、state digestに含めない。

### 4.4 canonical emitter

canonical emitterは検証済みsemantic ASTまたはtyped modelだけを入力とし、次の場合だけ使う。

- 新規fileまたはfragmentの生成
- journalからのexport
- clipboard textの生成
- ユーザーが明示した「文書を整形」
- semantic/golden test用の正規形生成

現行版の出力規則は次のとおりとする。

- BOMなしUTF-8、LF、末尾改行一つ、trailing whitespaceなし
- indentationはASCII space 4個
- section順は4.2の順。input、parameter、binding、program statement、assetの宣言順を保持する
- record fieldはschema registryの`canonical_order`順。必須fieldは常に出力する
- optional fieldがdefaultと同じで存在自体に意味がない場合は省略する
- `none`とfield省略をschemaが同義とする場合は省略へ正規化する
- 空recordは`{}`。非空recordは一field一行。空listは`[]`。非空listは一要素一行でcommaを付ける
- integerは先頭zeroと負zeroを持たない。decimalは先頭zeroを正規化し、小数部末尾zeroを除くが
  最低一桁を残す。typed Q16はjournal/catalogから生成する場合`q16(raw_i64)`を使用する
- stringは`\"`、`\\`、`\n`、`\r`、`\t`だけを短いescapeで出力し、それ以外のescape必須scalarは
  最短の小文字hex `\u{...}`を使用する
- UUIDとdigestは小文字canonical spellingとする。Base64 literalは開始delimiter直後にLFを置き、payloadを
  RFC 4648 canonical alphabet/paddingの最大76文字単位で、literal開始行のindentationより4 space深く
  一行ずつ出力する。payloadが空ならpayload行を出力しない。終了delimiterはliteral開始行と同じ
  indentationの新しい行へ置く
- commentを出力せず、行幅、locale、hash iteration順による条件分岐を行わない

同じsemantic ASTに対するcanonical byte列はOS、locale、thread数、hash iteration順にかかわらず
同一でなければならない。

## 5. 字句と構造文法

### 5.1 source characterとtrivia

sourceは妥当なUTF-8 byte列である。UTF-8 BOMはfileのbyte offset 0に一度だけ許可し、
それ以外のU+FEFFは通常のUnicode scalarとして扱う。NULはraw文字、escapeのいずれでも拒否する。
改行はLFまたはCRLFだけを許可し、CRLFを一つの改行として数える。単独CRはstring内の`\r`を
除いて拒否する。

```ebnf
trivia          = (space | tab | newline | line_comment)* ;
newline         = LF | CR, LF ;
line_comment    = "//", comment_scalar* ;
comment_scalar  = any Unicode scalar except NUL, CR, LF ;
space           = U+0020 ;
tab             = U+0009 ;
LF              = U+000A ;
CR              = U+000D ;
EOF             = end-of-source meta-terminal ;
```

line commentは次のnewlineまたはEOFの直前で終了し、comment token自身はnewlineを消費しない。
newlineは独立したtriviaとしてCSTに保持する。

tokenizerは常に最長一致を使用する。診断rangeは、callerから渡された元byte列先頭からのUTF-8
byte offsetによるhalf-open rangeを正本とする。先頭BOMはbyte offsetには含むが、表示用columnを
進めない。表示位置は1-based lineと1-based Unicode scalar columnとし、TABも一scalar、CRLFは
一改行とする。UTF-16 code-unit columnが必要なfrontendはbyte rangeから明示変換する。

### 5.2 token

word tokenはASCII小文字から始まり、ASCII小文字、数字、underscoreだけを含む。parserはcontextでkeyword
literalとidentifier roleを区別する。user-defined declaration、enum、constructor等のidentifier roleでは
予約keywordを拒否する。recordの`field_name`だけは予約keywordと同じspellingを許可するが、expected closed
schemaにそのfieldが存在しなければならない。これによりcommand record内の`enabled`等を表現できる。

```ebnf
word             = lower, (lower | digit | "_")* ;
identifier       = word ;
field_name       = word ;
lower            = "a" | ... | "z" ;
upper            = "A" | ... | "Z" ;
digit            = "0" | ... | "9" ;
nonzero_digit    = "1" | ... | "9" ;
hex_digit        = digit | "a" | ... | "f" | "A" | ... | "F" ;
unsigned_integer = "0" | nonzero_digit, digit* ;
integer          = "-"?, unsigned_integer ;
decimal          = "-"?, unsigned_integer, ".", digit+ ;
```

`+`、digit separator、exponent、hexadecimal、octal、leading zeroを許可しない。`-0`と`-0.0`は
読み取ってよいがcanonical emitterは`0`、`0.0`へ正規化する。decimal tokenを複数tokenへ分割して
解釈してはならない。

stringはJSONではなく、次のInkScript quoted stringである。

```ebnf
string          = '"', string_item*, '"' ;
string_item     = unescaped_scalar | escape ;
escape          = backslash, ('"' | backslash | "n" | "r" | "t")
                | backslash, "u", "{", hex_digit, hex_digit?, hex_digit?,
                                      hex_digit?, hex_digit?, hex_digit?, "}" ;
backslash       = U+005C ;
unescaped_scalar = any Unicode scalar except '"', backslash, NUL, CR, LF,
                   and U+0000 through U+001F ;
```

`unescaped_scalar`は`"`、`\`、NUL、CR、LF、U+0000..U+001Fを除くUnicode scalarである。
`\u{...}`は1～6桁のhexとし、surrogate、NUL、Unicode最大値を超える値を拒否する。UTF-16
surrogate pairとして解釈しない。

UUID、digest、Base64はcompound literalであり、prefixとdelimiterの間にtriviaを許可しない。

```ebnf
uuid_literal    = "uuid", string ;
digest_literal  = "blake3", string ;
base64_literal  = "base64", '"""', base64_body, '"""' ;
base64_body     = base64_item* ;
base64_item     = upper | lower | digit | "+" | "/" | "=" | space | tab | newline ;
```

UUIDは小文字hyphenated canonical UUID、digestは小文字64桁hexだけを受理する。Base64 bodyは
RFC 4648 alphabet、`=`、ASCII space、TAB、LF、CRLFだけを許可する。ASCII whitespaceを除去した後、
長さ、padding位置、未使用bitのzeroを検査する。Base64 body内でcommentを開始しない。

次は予約keywordで、declaration、enum、constructor名として使用できない。

`inkscript`、`inkscript_fragment`、`requires`、`meta`、`inputs`、`parameters`、`bindings`、
`program`、`output`、`execution`、`assets`、`file`、`folder`、`current_document`、
`current_sequence`、`param`、`let`、`select`、`assert`、`step`、`as`、`enabled`、`invoke`、
`editor_group`、`asset`、`true`、`false`、`none`、`uuid`、`blake3`、`base64`、`list`、`nullable`。

### 5.3 構造grammar

triviaはcompound literal内部を除くtoken間で無視する。

```ebnf
file             = file_header, file_section*, EOF ;
fragment         = fragment_header, fragment_section*, EOF ;
file_header      = "inkscript", unsigned_integer, ";" ;
fragment_header  = "inkscript_fragment", unsigned_integer, ";" ;

file_section     = requires | meta | inputs | parameters | bindings
                 | program | output | execution | assets ;
fragment_section = requires | parameters | bindings | program | assets ;
requires         = "requires", record ;
meta             = "meta", record ;
output           = "output", record ;
execution        = "execution", record ;

inputs           = "inputs", "{", (input_decl | input_profile)*, "}" ;
input_profile    = "profile", "=", ("canonical" | "batch"), ";" ;
input_decl       = "file", string, record?, ";"
                 | "folder", string, record?, ";"
                 | "current_document", record?, ";"
                 | "current_sequence", record?, ";" ;

parameters       = "parameters", "{", parameter_decl*, "}" ;
parameter_decl   = "param", identifier, ":", type_ref, "=", value, record?, ";" ;
bindings         = "bindings", "{", binding_decl*, "}" ;
binding_decl     = "let", identifier, "=", "select", identifier, record, ";" ;

program          = "program", "{", program_stmt*, "}" ;
program_stmt     = assert_stmt | step_stmt ;
assert_stmt      = "assert", identifier, record, ";" ;
step_stmt        = "step", string, ("as", identifier)?, "{", step_member*, "}" ;
step_member      = "enabled", "=", boolean, ";"
                 | "editor_group", "=", string, ";"
                 | "invoke", identifier, record, ";" ;

assets           = "assets", "{", asset_decl*, "}" ;
asset_decl       = "asset", identifier, record, ";" ;
record           = "{", field*, "}" ;
field            = field_name, "=", value, ";" ;

value            = boolean | integer | decimal | string | uuid_literal | digest_literal
                 | base64_literal | none | reference | asset_reference | constructor
                 | enum_literal | list | inline_record ;
boolean          = "true" | "false" ;
none             = "none" ;
enum_literal     = identifier | "folder" ;
constructor      = identifier, "(", argument_list?, ")" ;
argument_list    = value, (",", value)*, ","? ;
asset_reference  = "asset", "(", identifier, ")" ;
reference        = "$", identifier, reference_segment* ;
reference_segment = ".", field_name | "[", unsigned_integer, "]" ;
list             = "[", (value, (",", value)*, ","?)?, "]" ;
inline_record    = record ;
type_ref         = identifier | "list", "<", type_ref, ">"
                 | "nullable", "<", type_ref, ">" ;
```

sectionは任意順でparseするが同名sectionを拒否する。完全fileでは`requires`、`inputs`、
`program`、`output`、`execution`、fragmentでは`requires`と`program`を必須とする。

`step`は`enabled`と`invoke`を正確に一つ、`editor_group`を最大一つ持つ。member順はparse時には
任意だが、重複と欠落を拒否する。`configure_each_run`は実行構文に含めず、実行ごとの値は
parameterの`ask = each_run`だけを正本とする。`editor_group`は非空local keyで、一つのkeyはfile内の
一つのgroupだけを表す。同じkeyはそのmember stepに繰り返し記述できるが、全出現がprogram内で
連続しなければならず、離れたgroupの暗黙mergeを禁止する。groupは実行意味、dependency、Commit境界を変えない。

record field順は実行意味を持たず重複fieldを拒否する。input、parameter、binding、program
statement、list elementの順序は意味を持つ。asset declaration順は実行意味を持たないが、lossless
CSTとsource-preserving editでは保持し、generated fragmentでは最初の参照順と`AssetId`で正規化する。
parserはrecovery付きで複数診断を返してよいが、
一件でもerrorがあればCST以外の実行可能表現を公開してはならない。

## 6. 値と型

### 6.1 primitive value

| 型                                                | syntax / 範囲                                                  |
| ------------------------------------------------- | -------------------------------------------------------------- |
| `bool`                                            | `true` / `false`                                               |
| `u32`                                             | `0..4294967295`                                                |
| `i32`                                             | `-2147483648..2147483647`                                      |
| `u64`                                             | `0..18446744073709551615`。stable ID は別途 nonzero 制約を持つ |
| `i64`                                             | `-9223372036854775808..9223372036854775807`                    |
| `q16`                                             | decimal または `q16(raw_i64)`                                  |
| `string`                                          | InkScript quoted UTF-8 string                                  |
| `mask8` / `gray8` / `gray16` / `rgba8` / `rgba16` | native depthを保持するexact pixel value                        |
| `pixel_value`                                     | 上記exact pixel valueのclosed sum。variantを消去しない         |
| `point`                                           | `point(q16, q16)`。連続document pixel座標                      |
| `pixel_rect`                                      | `rect(i32, i32, u32, u32)`。half-open document pixel rect      |
| `uuid`                                            | canonical lowercase hyphenated `uuid"..."`                     |
| `digest`                                          | lowercase 64-hex `blake3"..."`                                 |
| `list<T>`                                         | `[value, ...]`                                                 |
| closed record                                     | `{ field = value; ... }`                                       |
| `nullable<T>`                                     | `T`または`none`。schemaが許可したfieldだけ使用可能             |

色 literal は次を使用する。

```inkscript
mask8(255)
gray8(128)
gray16(32768)
rgba8(255, 0, 0, 255)
rgba16(65535, 0, 0, 65535)
```

各constructorのarityとchannel範囲はregistryでexactに定義する。異なるnative depthの暗黙変換、
`pixel_value`から特定variantへの暗黙narrowingは禁止する。commandが特定formatを要求する場合、
実行対象、binding、literalの型不一致をcompileまたはbinding時に拒否する。

### 6.2 decimal の決定的変換

- exponent、`NaN`、infinity、hexadecimal float、locale 区切りを禁止する。
- decimal は符号、整数部、10進小数部から正確な有理数として読み取る。
- command schema が Q16 document scalar を要求する場合、`value * 65536` を
  ties-to-even で丸め、検査付き `i64` にする。
- unit interval、turn、pressure 等は command catalog が定める既存 canonical conversion
  を使用し、script parser が別の丸め規則を実装しない。
- `q16(raw)` は canonical raw 値を直接指定する上級者向け syntax であり、範囲検査を
  免除しない。

### 6.3 string escape

`\"`、`\\`、`\n`、`\r`、`\t`、`\u{1..6 hex digits}` を許可する。それ以外の
escape、NUL、invalid scalar を拒否する。path、name、label は byte limit と component
規則をそれぞれ追加検証する。

### 6.4 型の形成とliteral解決

現行版にuser-defined type、alias、generic function、implicit castはない。`type_ref`の
identifierは合成済み`SchemaView`に登録されたbuiltinまたはnamed closed typeへexact-current catalogで
解決する。`language-v3.json`はcommand非依存のbuiltin、stable ID reference、共有enum/record、asset
reference、selector referenceだけを所有する。commandの引数/result専用enum、record、constructorは
そのcatalog entryが所有し、language registryへ逆流させない。

integer tokenは符号付きの数学的整数、decimal tokenは正確な10進有理数として読み、expected
typeへ範囲検査付きで変換する。expected typeなしに整数型を推測せず、decimalを固定幅値へ変換
するのはregistryが指定するcanonical conversionだけとする。enumはexpected closed enumのmemberと
完全一致しなければならない。constructor nameはregistry内でglobalに一意とし、argument count、
argument type、result typeをexactに検査する。合成後のtype名とconstructor名は全entryを通じてglobalに
一意でなければならず、registry検証時に重複を拒否する。曖昧なoverload解決を許可しない。

`list<T>`はinvariantで、single valueとの暗黙昇格・降格を行わない。closed recordは未知field、
重複field、欠落required fieldを拒否する。field省略、default、`none`は次のとおり区別する。

- optional fieldの省略はschemaのdefaultを適用する
- `nullable<T>`だけが`none`を受理する
- 省略と`none`が異なる場合はregistryが両方の意味を明記する
- 両者が同義の場合、canonical emitterはfield省略へ正規化する
- 省略、明示defaultが同じtyped invocationになるschemaではsemantic digestも同じになる

### 6.5 symbol namespaceと可視性

`$name`で参照するparameter、binding、step result aliasは一つのvalue namespaceを共有する。
同名宣言、shadowing、予約keywordを拒否する。assetは別のasset namespaceを持ち、
`asset(name)`だけで参照する。`editor_group` keyはreference不能なfile-local group namespaceを持つ。
labelはいずれのnamespaceにも入らない。

- parameter defaultはliteralだけで構成し、referenceとasset referenceを含めない
- binding selectorは全parameterと、先に宣言されたbindingだけを参照できる
- assertとstepは全parameter、全binding、先行stepのresultだけを参照できる
- assetは宣言位置によらず参照できるが、compile前に全宣言を一意化する
- forward referenceと循環dependencyを拒否する
- disabled stepのresultを参照するsourceはcompile errorとする
- disabled step自身も名前解決、型検査、resource検査の対象とする

referenceの`.field`はclosed result/recordに存在しなければならない。`[index]`は`list<T>`または
registryが定めたtupleだけに使い、sourceに書かれた非負integer literalに限る。固定長型はcompile時、
可変長resultはitem実行時にboundsを検査し、範囲外を`missing_result`としてitem failureにする。
selectorの`all`は`list<entity_ref>`、`one`と`first`は`entity_ref`を生成し、command expected typeと
exactに一致しなければならない。

### 6.6 resultとdependency

各catalog result fieldは、scalar/fixed tuple/ordered list、型、stable ID namespace、owner role、
canonical element order、`always_on_success`または`only_on_change`のavailability、empty listの意味を
定義する。stable ID resultは`CanonicalProcedure.output_ids`上の開始ordinalと長さを持ち、全ordinalを
重複なくちょうど一度roleへ対応付ける。

`always_on_success` resultはsemantic no-opでも値を返す。`only_on_change` resultはno-op時に存在せず、
参照したconsumerをitem単位の`missing_result`にする。dependency graphはparameter、binding、result、
assetの全reference edgeを含む。fragment closure、`skip_dependents`、diagnosticは同じgraphを使う。

## 7. section 仕様

### 7.1 `requires`

```inkscript
requires {
    procedure_catalog = 8;
    replay_epoch = 29;
}
```

両 field は必須である。実行環境の exact-current 値と一致しなければ、入力 file や
asset を読む前に拒否する。未知 field、重複 field、0、範囲外を拒否する。

`.inkpod` top-level version は script に固定しない。reader と writer は常に実行環境の
exact-current native format を使用する。primitive semantics が変わる場合は、既存契約に
従い replay epoch と `.inkpod` top-level version を更新し、同時に InkScript catalog または
file version を必要に応じて更新する。

### 7.2 `meta`

```inkscript
meta {
    name = "Color cleanup";
    description = "Replace approved colors and resize cells.";
    extensions = [
        { key = "org.example.review-note"; value = "approved"; },
    ];
}
```

`name` と `description` は任意で、実行意味を持たない。source document、event range、
生成元 UI 等の provenance は将来の既知 metadata field として追加できるが、selector、
precondition、asset の代用にはならない。既知field以外のtop-level metadata fieldは拒否するが、
`extensions`は`{ key: string, value: string }`のbounded listとして未知keyをlossless round-tripする。
keyはreverse-DNS形式のASCII string、valueは実行意味を持たないUTF-8 stringだけに限る。reference、asset、
constructor、enum、number、record、listを許可しない。同じkeyを重複させず、実行、semantic digest、
portability判定へ使用しない。より豊かなmetadata値は別versionで専用型とcanonical orderを定義する。

### 7.3 `inputs`

```inkscript
inputs {
    profile = batch;
    file "cells/A001.inkpod";
    folder "cells/sequence-02" {
        cells = range(10, 0);
        recursive = false;
    };
    current_document;
}
```

`profile` は section 内に一個までの `canonical | batch` とし、省略は `canonical`。
command や output から推測しない。canonical emitter は既定の `canonical` を省略し、
`batch` は input declaration の前に出力する。四処理 pane の source は `batch` を明示する。

input kind は `file`、非再帰 `folder`、発行時 `current_document`、`current_sequence` の閉じた集合。
file／folder は native `.inkpod`、PNG、TIFF、TGA、BMP を共有 decoder で読む。
`current_sequence` は `canonical` だけで受け入れ、通常の runtime Sequence catalog の membership を
発行時に固定する。Cut descriptor や永続 membership は導入しない。

| 観測点 | `batch` | `canonical` |
| --- | --- | --- |
| item 順 | input 宣言順。folder 内だけ既存 Batch `natural_cmp` 順、同値は生 UTF-8 filename／canonical path key 順 | 全展開後の global natural order。下記 comparator を使う |
| 重複 | 同一 file identity／alias を拒否。active の重複宣言は保持し、native UUID だけでは重複としない | native UUID 重複または file alias を拒否 |
| `cells` | `all`／`range(first,last)`。各端の 0 は境界なし。非zero 両端が逆順なら拒否。stem の末尾数字runで比較し、数字なし／overflow は保持。active の range は構造検証後に無視 | nonzero の閉じた inclusive display-number range。逆順は拒否。current_document は `all` だけ |
| pathless active | label は `active-document.inkpod`、stem は `active-document` | label は `current-cell.inkpod`、stem はなし |
| open active snapshot | active 出力は完全な staged Core。その他の出力と画像 preview は document/assets から履歴/editor を再構成 | Genesis、asset、journal/branch、history cursor、全 namespace の ID high-watermark、document/editor と両 savepoint を保持 |
| open session が所有する file／folder input | disk を読み、未保存 live 編集を混入しない | open session の immutable snapshot を使い、dirty 内容を disk へ読み替えない |

canonical comparator は UTF-8 byte 列を左から比較する。両側が ASCII digit なら連続 run の先頭 zero を
除いた桁数、numeric digit bytes、元 run 長の順。その他の byte は ASCII だけ lowercase 化して比較する。
同値は元 UTF-8 label bytes、canonical path key、document UUID bytes の順で確定する。
Unicode normalization、locale、OS case-fold を使わない。profile が定める item ordinal を
output numbering、Stop、report、wait の順序に共用する。

file-backed item の label は authorized final filename、stem は最終拡張子を除く非空 component、
path order key は adapter が返す lossless UTF-8／`/`区切り／absolute root-tag 付き比較値とする。
canonical の pathless input は `duplicate` の空 basename、`cell_folder = true`、folder template の
`{stem}` を解決できないため拒否する。`new_save` の空 basename と `{index:4}` だけの template は使用できる。

raster input は file fingerprint と ingestion snapshot を束縛する。batch は既存 Batch の encoded bytes 由来
UUID、canonical は Rust が発行して plan 内で固定する非zero identity を使う。同じ bytes の別fileを
native UUID 重複と扱わない。canonical raster の表示番号は stem の末尾数字run、数字なし／0／overflow は1。
別 job 間の import identity 一致は保証せず、同じ plan の再実行と canonical pixel 結果の決定性を保つ。

relative path は保存済み `.inkscript` の親、未保存なら frontend が明示した base authority を使う。
暗黙 cwd、`~`、環境変数、wildcard／shell 展開は行わない。absolute path も authority と計画確認の対象。

発行時 `CommandContext` は session／sequence ID と generation、document／editor revision、UUID、
state digest、membership を固定する。PlanTask は Core owner thread で照合し、profile に従う immutable
snapshot を capture する。不一致を `stale_input` とし、別の active 文書へ再解決しない。
この capture を `plan snapshot time` と呼び、issue time と区別する。live path authority は移管しない。
詳細と Batch 比較上の明示差は [D2 接続契約](docs/inkscript-batch-connection.md#d2input-profile-と-source-例)に従う。

### 7.4 `parameters`

```inkscript
parameters {
    param replacement: pixel_value = rgba8(0, 64, 255, 255) {
        label = "Replacement color";
        ask = each_run;
    };
    param target_width: u32 = 1920 {
        label = "Width";
        ask = never;
    };
}
```

- parameter は immutable で、同名を禁止する。
- `ask` は `never` または `each_run`。既定は `never`。
- default value は必須のclosed literalで、referenceとasset referenceを含めず、declared typeと
  一致しなければならない。
- `each_run` は stored default を変更しない。毎回、全 parameter を解決した transient run
  copy を一件作り、その copy だけを enqueue する。
- Cancel、invalid、未解決 parameter では job を作らない。dry-run も未解決 parameter を
  許可しない。
- parameter は `$replacement` のように参照する。parameterからdocument stateを参照しない。

`ask = each_run`は実行ごとの設定の唯一の正本である。interactive frontendはstored defaultを初期値に
全該当parameterを提示し、non-interactive callerも各値について`accepted_default`またはoverrideを
明示したimmutable `ScriptRunParameters`を渡す。単にdefaultが存在することを解決済みとみなさない。
operationまたは`editor_group`の編集可能fieldに実行ごとの設定を設ける場合は、typed parameter群へ
loweringし、stepに別の実行flagを持たせない。このlanguage機能を現行Batch paneへ公開する判断はM1で行う。

### 7.5 `bindings` と selector

```inkscript
bindings {
    let paint = select plane {
        plane_kind = color;
        name = "Paint";
        cardinality = one;
        missing = error;
    };
}
```

binding は入力一件を staged Core へ読み込んだ後、最初の mutation より前の initial document
state に対して上から順に解決し、以後固定する。program 実行後の状態を selector で再検索しては
ならない。step が作成した object は step result 変数を使用する。

現行版の selector entity は次の閉じた集合とする。次表は概要であり、exact field、型、
required/default、owner relation、initial-order規則はschema registryと生成referenceを規範とする。

| entity                                 | 主な filter                                                                   |
| -------------------------------------- | ----------------------------------------------------------------------------- |
| `layer`                                | name、initial document order、persistent ID                                   |
| `plane`                                | owning layer binding/filter、plane kind、format、name、initial order、persistent ID |
| `guide`                                | axis、position、persistent ID                                                 |
| `shooting_frame`                       | document-owned singleton                                                      |
| `saved_selection_mask`                 | document-owned、name、persistent ID                                           |
| `light_table_set` / `light_table_item` | owner、name/order、persistent ID                                              |

共通 field は次のとおりとする。

- `cardinality = one | first | all`。既定は `one`。
- `missing = error | skip_dependents`。既定は `error`。
- `persistent_id` を指定する場合は、同じ selector に `source_document_uuid` も必須とし、
  UUID が異なる入力への偶然の ID 一致を拒否する。
- `one` は0件または複数件を拒否する。`first` はinitial document orderの先頭を選ぶ。
  `all` はcommand schemaがlist referenceを受理する場合だけ許可する。
- name だけの selector は許容するが、重複時に暗黙で先頭を選ばない。
- `skip_dependents` は同じdependency graph上で、そのbindingに直接または推移的に依存する
  assert/stepをskippedとしてreportする。skipされたproducerのresultを参照するconsumerにもskipを
  伝播し、`missing_result`へ変換しない。明示referenceを持たない後続stepは実行するため、catalogの
  editor metadataはoptional処理としてskipしてよいstepだけにこのpolicyを提示する。selector ambiguityは
  skipせずerrorとする。

raw stable ID、array index、現在 active な object への暗黙再解決は禁止する。履歴から export
した strict ID binding は batch UI の rebind 操作で semantic selector へ置換できる。

### 7.6 `program`

```inkscript
program {
    assert document {
        width = 1920;
        height = 1080;
    };

    step "Replace approved colors" {
        enabled = true;
        invoke replace_raster_colors {
            plane_id = $paint;
            pairs = [
                {
                    enabled = true;
                    old = rgba8(255, 0, 0, 255);
                    new = $replacement;
                },
            ];
        };
    }
}
```

`program` は記述順に実行する。implicit parallelism、reordering、dead-step elimination は
禁止する。disabled step は検証とround-tripの対象だが実行しない。

一つの `step` はちょうど一つの typed invocation を持ち、成功して実変更があれば一つの
通常 journal Commit を生成する。semantic no-op は Commit、revision、history、dirty、IDを
進めない。no-op step の後も後続 step は続行する。

一般InkScriptでContinuous Fillを表す場合はseedをsource順に一seed一stepへ展開し、enabled seedのstepだけが既存fill
primitiveを一回呼ぶ。disabled seedは`enabled = false`のstepとして保持でき、実行もCommit生成もしない。
enabled seedがN件ならoperation全体で0..N Commitを生成する。UI上の一operationというまとまりは、連続するstepが共有する非意味的な
`editor_group`で保持する。将来のstructured editorでこのまとまりを編集する場合も、grouped step列と
canonical invocation列のlosslessな対応を検証する。逆変換は同一groupの全stepが同じtarget/configを持つ
seed-fill等、registryがlossless projectionを定義した場合だけ許可する。
現行の`BatchOperation`型との変換が実装済みという意味ではなく、一般scriptを四処理のpresetへ丸めてはならない。

この1:N契約を現行Batch v5の四処理へ流用しない。v5では全enabled処理と展開済みtargetを一つの
`ApplyBatchOperations`へ渡し、一回だけcommitする。四処理を別々のstepへ展開したり、非意味的な
`editor_group`でtransactionを合成したりして同等と扱わない。catalog v8 の
`apply_batch_operations { operations: list<batch_operation>; }` が四 variant の順序付き列を表す。
closed field、role／strict／references、disabled 参照、上限、export／rebind は
[承認済み D1 契約](docs/inkscript-batch-connection.md#d1一-command-の表現と実行)に従う。
既存 record/list grammar を使い、catalog-owned の conditional field と list bound を検証する。
全 operation 無効の draft は保存可能だが実行不能。role／strict は initial input へ固定し、
先行 producer の references と実行直前の dimensions／全列 resource を再検査する。

新規 stable object を作成する command は typed result を返せる。

```inkscript
step "Create paint layer" as created_paint {
    enabled = true;
    invoke create_layer {
        name = "Paint";
    };
}

step "Rename created layer" {
    enabled = true;
    invoke set_layer_properties {
        layer_id = $created_paint.layer;
        visible = true;
        editable = true;
        opacity_milli = 1000;
        name = "Final Paint";
    };
}
```

`create_layer`はlayer kindを受け取らず、MainLine 1枚とColor 1枚を必須とする標準layerを作る。
追加の`create_plane { layer_id; format; name; }`は常にRaster roleを作成する。
`convert_plane { plane_id; destination_format; }`は既存のMainLine／Color／Raster roleを変えず、
pixel formatだけを変換する。layer変換commandとplane role変換commandは存在しない。

current selectionはdocumentの一時maskであり、再利用するmaskはdocument-owned collectionへ保存する。

```inkscript
step "Save approved region" as saved {
    enabled = true;
    invoke save_selection_mask {
        name = "Approved region";
    };
}

step "Add approved region" {
    enabled = true;
    invoke apply_saved_selection_mask {
        saved_selection_id = $saved.saved_selection_mask;
        operation = add;
    };
}
```

保存maskの再命名と削除は`rename_saved_selection_mask`、`delete_saved_selection_mask`を使う。
`apply_saved_selection_mask`のoperationは`replace | add | subtract`の閉じた集合である。

result fieldと型はschema registryが定義し、scalar、list、roleを失わない。例えば
`edit_targets`の結果を`edited`と名付けた場合の`$edited.planes[0]`のようなconstant indexを許可する。IDはcommit成功時だけ消費する。
disabled producerへの参照はcompile error、`skip_dependents`によるskipped producerへの参照はconsumerも
skip、`only_on_change` resultをno-op/failure後に参照した場合だけ、そのinput itemを
`missing_result`として失敗させる。

`assert` は mutation と Commit を生成しない。現行版は次を持つ。

- `assert document`: UUID、state digest、ID allocation digest、寸法、DPI、色空間等の既知field
- `assert object`: binding reference と既知 property
- `assert selection`: empty/nonempty と half-open bounds

assert failure はその入力 item を変更せず失敗させる。汎用 boolean expression、条件分岐、
loop は現行版では提供しない。入力一件ごとの反復だけが暗黙の bounded loop である。

`id_allocation_digest`は全persistent-ID namespaceをregistryのnamespace tag順に並べ、各
`(namespace_tag, next_nonzero_id)`をdomain-separated BLAKE3へ入れた値とする。削除済みIDを含む
high-watermarkを表し、document state digestで代用しない。exact-source fragmentはprogram先頭の
`assert document`でsource UUID、base state digest、ID allocation digestを必須検査する。

ID allocation digest algorithm v1のdomain contextはASCII `inkpod.inkscript.id-allocation-digest.v1`とし、BLAKE3 derive-key modeを
使う。hash inputは`namespace_count: u32_le`に続き、registry順で
`tag_length: u16_le`、ASCII tag bytes、`next_nonzero_id: u64_le`を連結する。tagは重複不可で、count/length
overflow、zero/overflow済みnext IDをdigest計算前に拒否する。

### 7.7 InkScript schema registry

現行のregistry schema／language／catalog／owner manifestは1節の参照先を正本とする。
script公開対象のjournal-replayable `PrimitiveId`はowner manifestのちょうど一つのownerへ割り当て、
対象外は理由付きで明示する。`ApplyBatchOperations` は catalog v8 の一 command として含む。
native catalog の既存 private flag は保存済み replay fingerprint の一部として維持し、
Script への公開集合は script registry／owner manifest で定義する。payload／replay semantics は変更しない。
合成`SchemaView`はlanguage-core定義と全catalog entry定義を結合し、type/constructor名の重複を拒否する。
全entry、実装、owner、equivalence evidenceの全単射を検証する。既存C ABIの所有権契約は維持し、
製品file、clipboard、Windows commandへの接続はM15のcutoverまで行わない。

registryは最低限、次を定義する。

- builtin、named enum、constructor、closed record、nullable、list type
- file/fragment section、input kind、parameter metadata、output、execution、asset descriptor
- selector entity、exact filter、owner relation、initial-order、cardinality、missing policy
- assert kind、field、比較方法
- commandのstable snake_case name、対応`PrimitiveId`、primitive schema、replay epoch、semantics revision
- argumentの名前、型、presence、default、nullable、bound、canonical order
- stable-ID input role/namespace、asset role/descriptor、inline/external許可
- result fieldの型、availability、cardinality、namespace、`output_ids` ordinal、canonical order
- 実引数とdependencyから算出するportability evaluatorとrequired precondition
- checked `max_invocations`、`max_output_ids`、`max_asset_bytes`、`max_work_units`、
  `max_output_growth`、cancellation boundary
- Batch editor/history rebind UI用の非意味的editor metadata
- entryを実装する唯一のmilestone IDとequivalence test ID

registry内のportability ruleとwork formulaはexact-current `registry-schema-v2.json`が定めるclosed JSON ASTで表し、
任意expression string、Rust function/type名、source codeを格納しない。numeric expressionはbounded typed
argument/input/asset summary fieldの読取り、`u64` literal、list length、checked add/subtract/multiply、
nonzero ceil-divide、min/max、signed値のchecked absolute、compile-time上限付きlistのbounded sumだけを
許可する。boolean expressionはtyped equality/order、and/or/notを許可し、conditionalはboolean predicateと
同じ型の二branchを持つ。評価順はJSON ASTのchild順、and/or/conditionalだけをleft-to-right short-circuitと
する。missing field、type mismatch、zero divisor、overflow、unknown operator、上限なしiterationはcatalog
validation errorであり、saturateまたはimplementation callbackへfallbackしない。

portability evaluatorは上記boolean ASTを`when`に持つ順序付きrule列と、必須のfinal defaultで表す。
最初に一致したruleだけが`portable | requires_binding | strict_source_only`とclosed required-precondition集合を
返す。work formulaは各resource metricに一つのnumeric ASTを持ち、同じtyped summaryなら全実装で同じ値を
返さなければならない。JSON nodeのexact field、tag、path表現、depth/node上限は現行registry schemaを正本とする。

portabilityはcommand名だけの固定値ではない。純粋な
`evaluate_portability(typed_arguments, input_roles, asset_roles)`が次のclassとrequired preconditionを
返す。source authorはclassを指定または緩和できない。

- `portable`: source固有ID/state authorityを必要としない
- `requires_binding`: semantic selectorまたは先行resultへの明示binding後に実行できる
- `strict_source_only`: exact source UUID、base state digest、ID allocation digest等が必須で、現行versionでは
  別文書へrebindできない

classの強さは`portable < requires_binding < strict_source_only`とし、step、group、fragmentは全dependencyの
最大値を取る。fragment内で閉じた先行resultや取り込み済みcontent-addressed assetはclassを強めない。
runtime active object、外部path、clock等への暗黙依存はclassでは正当化できずcatalog errorとする。

Rust variant名、`Debug` output、frontend command ID、localized labelから実行名やfieldを生成しない。
未知command、assert、selector、constructor、enum、fieldは推測せず拒否する。各`PrimitiveId`とprimitive
schemaの組はちょうど一つのentryに対応し、一つのscript commandを複数primitiveの暗黙transactionへ
loweringしない。registry、生成reference、Rust declaration、primitive catalogのdriftをCI failureとする。

次は script 化しない。

- replay-policy が journal-replayable でない primitive
- query、view、preview-only、ingestion-only、export、save/open
- `HistoryMove`、`BranchCut`、Genesis replacement
- frontend command ID

procedure の `expected_revision`、`base_state_id`、`committed_state_id`、procedure/event/branch ID、
pre/post digest は source argument にしない。必要な revision は各 staged execution point で
executor が取得する。exact-sourceのstrict preconditionはregistryに従って`assert`とselectorに明示する。

### 7.8 `assets`

canonical procedure が参照する immutable payload は content-addressed asset として宣言する。
次のdigestはlayoutを示す説明値であり、この断片単独を有効なasset fixtureとして使用しない。

```inkscript
assets {
    asset imported_raster {
        asset_id = blake3"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        kind = canonical_raster;
        descriptor = {
            pixel_format = rgba8;
            color_space = srgb;
            alpha = straight;
            width = 2;
            height = 2;
            stride = 8;
            element_count = 4;
        };
        data = base64"""
            /wAA/wD/AP8AAP////8A/w==
        """;
    };
}
```

一つの asset は `data` または `data_file` のどちらか一方だけを持つ。

- `data`: Base64でinlineしたcanonical logical payload。ASCII whitespaceを無視する。
- `data_file`: script directoryに対するrelative path、または明示許可されたabsolute pathにある
  raw canonical logical payload。encoded PNG/TIFF等ではない。
- `asset_id`: descriptorとlogical payloadから既存canonical asset digest規則で得た値。

各asset kindのrequired/optional field、descriptor type、logical byte layout、length式、unknown field規則は
schema registryでclosedに定義する。digest一致だけでkind/descriptor/length検査を省略しない。

compiler は全参照 asset を mutation 前に読み、長さ、descriptor、digest、重複、総量を検証し、
Rust-owned immutable asset store へ取り込む。実行中に path を再読込しない。clipboard fragment は
必ずinline `data`で自己完結させる。inline上限を超える場合はcopyを診断付きで拒否し、side fileを
暗黙作成しない。完全fileでside fileを作るauthoring操作は、保存先とauthorityを別UI transactionで
明示確定した後にだけ行う。

external assetはPlanTaskがauthority検証済みhandleからidentityとlengthを取得し、bounded stream読取の
前後で同じidentity/lengthを再検査する。途中変更は`stale_asset`としてplanを作らない。成功後はpathではなく
descriptorと`AssetId`をplan/confirmation digestへ含め、RunTaskはfreeze済みbytesだけを使用する。

外部の一般画像を読み込む authoring convenience は、将来 `ingest` 宣言として追加できるが、
現行版の canonical `asset` と混同しない。Coreへ渡る procedure は外部 pathを保持しない。

### 7.9 `output`

```inkscript
output {
    policy = duplicate;
    format = inkpod;
    folder = "output";
    cell_folder = false;
    basename = "painted";
    start_number = 1;
    direction = ascending;
}
```

file v3 の `output` は policy ごとの closed variant とし、別 variant の field、未知 field、
重複 field を拒否する。既存の native naming 三 policy は次の field と意味を保持する。

| field          | `duplicate` / `new_save`            | `explicit_overwrite` |
| -------------- | ----------------------------------- | -------------------- |
| `policy`       | 必須                                | 必須                 |
| `format`       | 必須、`inkpod`                      | 必須、`inkpod`       |
| `folder`       | 必須。空stringを許可                | 指定禁止             |
| `cell_folder`  | 必須bool                            | 指定禁止             |
| `basename`     | 必須。空stringを許可                | 指定禁止             |
| `start_number` | 必須`u32`                           | 指定禁止             |
| `direction`    | 必須、`ascending`または`descending` | 指定禁止             |

三つの native naming policy の destination derivation は次のとおりとする。

- `folder = ""`はfile-backed inputの親directory。pathless/in-memory inputではerror
- relative folderは保存済みscriptのauthorized parent、unsaved sourceでは明示base authorityを基準にする
- `cell_folder = true`ならbase folderの下にsource stemのsubdirectoryを置く
- `duplicate`かつ空basenameなら`<source_stem>_batch.inkpod`
- `duplicate`または`new_save`で非空basenameなら`<basename>_<number>.inkpod`
- `new_save`かつ空basenameなら`cell_<number>.inkpod`
- numberは最低4桁でzero-padし、大きな値をtruncateしない。`start_number +/- item_ordinal`はchecked
  `u32`とする。`item_ordinal`はimmutable previewの0-based ordinalであり、overflowまたはdescending
  underflowでplan全体を拒否してsaturateしない
- parent directoryはRunTaskのinstall段階だけで作る。dry-runとPlanTaskはfilesystemを変更しない
- install開始後に作成したdirectoryは、その後のfailure/cancelで空のまま残ることがある。この副作用を
  reportへ記録し、既存directoryをrollback名目で削除しない
- non-overwriteはinput alias、item間collision、script、asset、既存destinationとのcollisionを拒否し、
  自動renameしない。case-fold、reparse、file identityによるaliasも検査する

三policyはいずれもstaged documentのUUID、Genesis、journal、stable ID、asset identityを変更しない。
`duplicate`はlogical forkや新UUID、`new_save`はlive sessionのSave Asやpath authority移管を意味せず、
差は destination naming だけである。下記 `new_tabs` は新 identity／Genesis を作る別 variant とする。

`explicit_overwrite`はopen `DocumentSession`が所有していないnative file-backed input自身だけをdestinationに
できる。open sessionのbacking path、current document/sequenceのopen member、別input pathを拒否する。
source/destination identityとopen-session registry generationをplan時とinstall直前に再検査する。
overwriteにはsource上のpolicyだけでなく、planへ一回限りで束縛したpreview/confirmation tokenが必須である。

file identityだけの再検査では、同一identityのまま行われる外部writeを防げない。`explicit_overwrite`は、
planned fingerprintに対するno-lost-update guardをOS adapterが提供できるfilesystemだけで許可する。guardは
最終fingerprint検証からatomic replaceのlinearizationまで、同一identityへのcontent変更、truncate、rename、
delete、置換を排他するか確実に検出しなければならない。RunTaskはguard取得後にvolume/file identity、length、
content digest、native document UUID、利用可能なchange tokenを再検証し、planned値と違えば`stale_input`として
installしない。単なるcheck-then-replaceしか提供できないfilesystemではPlanTaskが
`unsupported_atomic_overwrite`として拒否する。raster input を native bytes で上書きしてはならない。

file v3 は M1 承認済みの次の三 variant も持つ。

| `policy` | 必須 field（`policy` 以外） | 契約 |
| --- | --- | --- |
| `folder` | `format`, `folder`, `naming_template` | format は `inkpod | png | tiff | tga | bmp`。非空 folder と bounded template から一 item 一 file を作る |
| `active_document` | なし | input は `current_document` 一宣言／一件。enabled mutation step は単一 `apply_batch_operations` だけで、assert は許可 |
| `new_tabs` | なし | 結果ごとに新 identity の pathless／dirty document を staged publication する |

folder は32,768 UTF-8 bytes以下。template は1–1,024 bytes、`{stem}` と `{index:N}`（N=1–12）だけを
placeholder とし、index は1始まり、extension はformatが決める。absolute path、separator、dot、`..`、
extension token を拒否する。既存 destination、input alias、item 間 collision を拒否し、自動 rename しない。
raster export は既存 common composite encoder を使う。RGBA16 の格納精度を新たに保証しない。
enabled outer step 内の enabled Batch masking と raster folder の組合せは実行前に拒否する。
入力に既存 mask があるだけでは拒否へ強化しない。

active 出力は発行時 session／generation と文書・editor・savepoint を公開直前に照合し、一 Undo として
適用する。path authority と両 savepoint は保持し、stale／cancel／invalid は live document を変更しない。
new_tabs は最大必要件数を開始前に capacity preflight し、結果の document/assets から history/editor を
再構成する。source、既存 session、同時公開結果と衝突しない新 identity を Rust が発行する。
source history を持ち越す logical fork ではない。両 staged result は owner に一回だけ移管し、
close／stale 時に別 session へ公開しない。詳細は [D3 接続契約](docs/inkscript-batch-connection.md#d3outputpreview失敗と所有権)に従う。

### 7.10 `execution`

```inkscript
execution {
    failure = continue;
    wait_ms = 0;
    preview_before_save = true;
}
```

- `failure = continue | stop`
- `wait_ms` は `0..3600000`
- `preview_before_save` は boolean

dry-run と `current/all` run scope は実行 command のoptionでありsourceへ永続化しない。
これにより、保存されたscriptを開いただけでoverwriteや実行modeが暗黙選択されることを防ぐ。

`preview_before_save = true`ではinteractive frontendがPlanTask後の`ExecutionPreview`を表示してから
confirmation tokenを発行する。`false`でもauthority、PlanTask、token、stale/collision検査を省略せず、
interactive frontendは直前の明示Run操作へ同じplanを束縛してtokenを発行できる。non-interactive callerは
同じplan digestへの明示承認を渡さなければならない。`explicit_overwrite`はこの値にかかわらず、replace
対象を列挙した明示確認を必須とする。

`all`はimmutable previewの全item、`current`はcommand発行時の`CommandContext`に固定したdocument UUID
またはfile identityと一致するitem一件だけを実行する。0件または複数一致をerrorとし、現在activeな
別itemへ再解決しない。scopeもconfirmation tokenへ含める。

## 8. 実行意味論

### 8.1 static compile、PlanTask、confirmation

実行開始前を次の五段階に分ける。

1. **Static compile**: source bytes、version、grammar、symbol/type、dependency、parameter値、
   aggregate syntax/work boundを検証する。外部pathをopen/列挙せず、read/enumerate/create/replaceの
   要求を列挙した`PathIntentPreview`、`static_compile_digest`、`path_intent_digest`を返す。
2. **Authority**: frontendが各intentへruntime-onlyの明示authorityを付与し、tokenを
   `static_compile_digest`、`path_intent_digest`、intent IDへ束縛する。script text自体をauthorityと
   みなさない。
3. **PlanTask**: progress/cancel付きでfolder列挙、current session snapshot、file fingerprint、asset
   ingestion、selector-independent output/work計画を作り、immutable `ScriptExecutionPlan`と正確な
   `ExecutionPreview`を返す。document、destination、savepointを変更しない。
4. **Confirmation**: plan digest、run scope、全input fingerprint、全asset descriptor/AssetId、
   全destination identity、policy、authority generationへ束縛した一回限りのtokenを作る。source、parameter、input、output、authority、
   session generationの変更で失効する。
5. **RunTask**: tokenを消費して再検証し、preview順にitemを実行する。

static errorや未解決authorityではPlanTaskを作らず、PlanTaskのfailure/cancelではRunTaskを作らない。
source parse、folder展開、asset読込をUI threadまたはcancel不能な長時間Core callで実行してはならない。
実行中のsource等を変更する場合は進行中taskをcancelし、新しいstatic compileからやり直す。

current inputについてtokenが参照するのはimmutable plan snapshot identityであり、snapshot取得後のlive
document revision変更だけでは失効しない。session close/replacementによるgeneration変更は失効する。
file/folder inputはplanned fingerprintからの変更で失効する。

file/folder itemはauthorized final path、利用可能なOSのvolume/file identity、length、content digest、
native document UUIDまたはraster ingestion identityを固定する。RunTaskはread直前とread後にidentity、length、digestを
再検査し、不一致を`stale_input`としてmutation前に拒否する。静かに最新fileへ読み替えない。

path authorityはfrontendが発行するruntime-only opaque tokenで、source/fragmentへserializeしない。
tokenはauthorized rootまたはexact object、`read | enumerate | create | replace` capability、generation、
`static_compile_digest`、`path_intent_digest`、intent IDを持つ。PlanTaskは検証済みauthorityのIDと
generationを含めてplan digestを生成し、confirmation tokenだけがplan digestへ束縛される。OS adapterは
authority後にhandle-basedでfinal targetを解決し、symlink/reparse
targetがauthorized root内であることを検査する。alias判定はfile identityを優先する。`..`、implicit
cwd、`~`、environment/wildcard/shell expansion、network URL、UNC pathは現行版で拒否する。Rust Coreは
opaque OS tokenを解釈せず、adapterが検証したbounded path/identity DTOだけを受け取る。

file出力のtemporaryもoutputの`create` authority外へ書いてはならない。画像previewは8.2.1の専用temporaryを使う。
RunTaskは作成直前にcancel、authority
generation、confirmation tokenを再検査し、検証済みdestination parent handle配下へhandle-relative、
no-follow、exclusive createで作る。名前衝突のretryはboundedとする。writer handleはwrite/flush後にcloseし、
その後は検証済みparent-directory handle、temporaryのrelative component、file identityを保持する。installまたは
cleanup直前にparent handle相対・no-followで非writing control handleを取得し直し、identityを照合して、
外部write、delete、renameをlinearizationまたはcleanup完了まで排他または検出する。string absolute pathから再openしない。
このguarded objectを条件にatomic install/cleanupできないfilesystemでは、adapterがtemporary作成前に
`unsupported_atomic_install`として拒否する。parent identity変更、reparse化、authority失効、temporary identity
不一致は`stale_destination`としてinstallせず、別identityをcleanupしない。

まだ存在しないdestinationのplan identityは、handleで解決した最も近い既存parentのvolume/file identity、
そこからの検証済みrelative component列、最終名、`expected_absent`を組にする。install直前に同じparent
identityとabsenceを再検査し、non-overwriteはatomic create-if-absent、overwriteは検証済み同一fileへの
atomic replaceだけを許可する。

missing intermediate componentは検証済みparent handleから一componentずつhandle-relative/no-followで
openまたはcreateし、各componentのsymlink/reparse pointを拒否する。stringでabsolute pathを再結合して
create/replaceしない。plan後にcomponentが出現した、identityが変わった、reparse化した場合は
`stale_destination`とする。最終atomic create/replaceも検証済みparent handle相対で行う。

PlanTaskは全destinationの共有directory graphを作り、expected existing/absent identityをtokenへ含める。
RunTask自身がgraphどおりに作成したdirectoryはjob-local graphへ実identityを記録し、後続itemはそのexact
identityを検証して再利用できる。外部主体が作成・置換したcomponent、または記録identity不一致だけを
`stale_destination`とする。

### 8.2 入力一件の transaction

各入力は独立したstaged Coreでpreview順に処理する。

1. current_document/current_sequenceはprofileに従う固定snapshot、file/folderはfingerprint再検査済みの
   authorized sourceからcurrent native readerまたは共有raster decoderでopenする。
2. initial stateでbindingsを解決する。
3. assertとenabled stepを順番に実行する。
4. 各stepを既存canonical executorへ渡す。
5. file出力は選択codecで完全encodeする。active／new_tabsは7.9のstaged resultを作る。
6. file出力ではcancellation、authority、confirmation tokenを再検査し、検証済みdestination parent handle配下の
   同一volume exclusive temporary fileをwrite/flush/closeする。
7. file出力のoverwriteではno-lost-update guard下の完全なsource fingerprint、全file policyではdestination identity、
   open-session registry、authority、confirmation tokenを再検査してatomic installする。

canonical profile の `CoreSessionSnapshot` は単なる`CellDocument` cloneから新Genesisを作らず、native open/cache-free replayと
同じvalidation経路でstaged Coreへ復元する。既存journal/history/allocatorへscript Commitをappendし、
UUID、Genesis、既存branch、savepointを保持する。native file出力だけが最終stateのprospective savepointを記録する。
batch profile の materialize と new_tabs の新Genesis化は7.3／7.9の別境界に従う。

任意の段階のinvalid、failure、cancel、stale、overflow、allocation failureで、その入力の
working Coreとexact temporary fileだけを破棄する。入力file、別item、live current document、
savepointを部分変更しない。`failure` policyは次itemを開始するかだけを決める。

atomicityはdestination file内容についてitem単位であり、job全体や7.9で明記した空directory副作用は
対象外である。atomic create/replace成功をitemのlinearization pointとし、その後に観測したcancelで当該itemを
`cancelled`へ戻さず`installed`と報告し、次item以降だけを`not_started`にする。後続itemのfailure/cancelで
install済みの先行itemをrollbackしない。reportはpreview ordinalごとに
`installed | staged | dry_run | failed | cancelled | not_started`を必ず一件持つ。
`failure = stop`は失敗itemの後を`not_started`にする。linearization前にcancelを観測したactive itemは
`cancelled`としてinstallせず、残りを`not_started`にする。linearization後は前段の規則に従う。
output `.inkpod`には最終stateを指すprospective document/editor savepointを書くが、source/live sessionの
savepoint、dirty、path authorityを進めない。

dry-runも同じparser、binder、asset ingestion、canonical executorをstaged Coreで実行するが、
encode用temporary fileを作らず、outputをinstallしない。単純なsyntax checkをdry-runと呼ばない。

### 8.2.1 画像 preview と staged publication

`PathIntentPreview`／`ExecutionPreview` は authority・入力順・衝突を確認する計画であり、画像ではない。
staged dry-run は隔離 Core の実行 report、画像 preview は保存・再読込後の contact sheet とし、
異なる結果型・副作用を持つ。画像 preview のために実 destination を作成・変更しない。

画像 preview は plan の全file inputをfingerprint照合してcopyし、全session snapshotを7.3のprofile規則で
materializeしてから最初のcommandを実行する。canonicalのopen dirty fileもsnapshotを使う。
copy完了後は隔離したbytesを正本とし、元fileが後から変化しても最新bytesへの読み替え・再copyを行わない。
origin session／authority の generation は公開直前まで照合し、別active文書へfallbackしない。
folder出力なら同じcodec、その他ならnativeでtemporaryに保存・再読込してthumbnailを作る。

専用temporaryはinput/output合計4 GiB以下。contact sheetは長辺160のthumbnail、padding8を基準に、
16,777,216 pixels以下へ収めたRGBA8とする。input順、失敗の赤系slot、Stop後の未処理灰色slot、
透明checkerboardを保持する。cleanup完了後にcancel／origin／authorityを再確認し、一つのclean／pathless
表示専用Coreと元の発行時contextを返す。cancel／stale／cleanup失敗は表示結果を返さない。

通常runの成功済み先行itemは後続failure／cancelでrollbackしない。new_tabsの完了staged itemも保持する。
active出力は公開直前のcancel／stale照合を要する。未取得staged resultはtask/ownerのreleaseで破棄し、
移管は一回限りとする。画像previewは全体で一つの表示結果のため、途中成功slotを単独公開しない。

### 8.3 journal と Undo

- 実変更した一つのstepは一つの通常Commit/Undo単位になる。
- no-op、disabled、skipped、assertはCommitを作らない。
- script全体を一つの巨大なprocedureへ包まない。
- output `.inkpod` は、scriptが生成した通常のcanonical Commit列と必要assetを保持する。
- script sourceやpathをprocedure payloadへ埋め込まない。任意のscript name/digest provenanceを
  持たせる場合も、document semanticsとreplay authorityから分離する。

### 8.4 決定性

結果はOS path列挙順、hash iteration順、locale、clock、thread数、GPU、UI stateに依存しない。
folder展開、selectorの`first/all`、asset、parameter、stepは明示的な決定順を持つ。
現行版のitem execution、encode、installはimmutable preview順の逐次実行に固定する。item並列化、
out-of-order completion/installを禁止する。`wait_ms`は一item終了後から次item開始前だけに適用し、
Core engine threadをsleep/blockせずtimer continuationでyieldする。immutable bytesのhash/encode等を
workerへ委譲しても、Core操作とinstall順を変えてはならない。将来のitem並列化はfile/catalog versionを
要する別仕様とし、ordered install barrierとstop/cancel時のlater result破棄を定義するまで実装しない。

## 9. journal からの export

履歴可視化画面の表示文字列は要約であり、executable source の入力にしない。exporter は、
可視化snapshot作成時に固定した exact `CanonicalProcedure`、typed runtime invocation、asset store、
state linkage から直接 InkScript fragment AST を作る。

選択Commit列を`C0..Cn`としたとき、exact-source replayの基準state `B`は`C0.base_state_id`が
指すparent state、期待state `F`は`Cn`のcommitted stateである。Genesis直後から選択した場合だけ
Genesis単独を`B`にできる。選択途中の断片をGenesisへ直接適用して同じ結果になるとはみなさない。

### 9.1 export 対象

- 一つの Commit は、catalog entry が存在すれば一つの `step` としてexportできる。
- 複数Commitは、選択順がjournal event順で、各commitのbase stateが直前commitのcommitted stateと
  一致する一つの線形祖先列である場合だけ一つのprogram fragmentにできる。
- branchをまたぐ、間に必要な未選択commitがある、またはstate linkageが連続しない選択を
  暗黙reorder/mergeしてはならず、具体的な行を示して拒否する。
- Genesis、HistoryMove、BranchCutはexportしない。

exporterはexact-source fragmentのprogram先頭へ次のpreconditionを必ず生成する。

```inkscript
assert document {
    source_document_uuid = uuid"00000000-0000-0000-0000-000000000000";
    state_digest = blake3"0000000000000000000000000000000000000000000000000000000000000000";
    id_allocation_digest = blake3"0000000000000000000000000000000000000000000000000000000000000000";
};
```

値はlayout説明用である。実行側はmutation前に三値とregistryが要求する追加preconditionを検査し、
不一致を`stale_precondition`としてitemを変更せず拒否する。このassertを削除・緩和したfragmentを
exact exportと表示しない。`strict_source_only` stepを含むfragmentでsource UUID、base state digest、
ID allocation digest、registry-required preconditionが欠落または緩和されている場合はcompile errorとする。
必要なpreconditionが完全ならexact-source fragmentとしてcompileできる。

### 9.2 ID 再束縛

- 選択範囲内の先行stepが生成したoutput IDは、registryのresult roleと`output_ids` ordinalから
  `$step.created_items[0]`等のresult referenceへ一意に変換する。対応不能ならexportを拒否する。
- 選択範囲外で既に存在したIDはstrict selector bindingとしてfragmentへ出す。
- strict selectorは`persistent_id`と`source_document_uuid`を含み、別文書では未解決になる。
- paste時のrebind UIは、kind、name、owner、format等のhintからsemantic selector候補を表示する。
- 候補が0または複数ならユーザーが明示決定するまでinsert/runを拒否する。
- raw IDを現在activeな別objectへ自動的に置換しない。

### 9.3 exact-sourceとreboundの検証

exact-source equivalenceは、同じreplay epoch/catalog/assetsの下で元文書を`B`までcache-free replayし、
fragment適用後に次が元の`C0..Cn`と一致することを意味する。

- `F`のdocument state/pixel digest
- 全stable-ID namespaceのhigh-watermarkとID allocation digest
- 各stepのtyped result role、cardinality、output ID ordinalと実際のstable output ID
- 各Commitの`pre_state_digest`と`post_state_digest`
- schema role順のstable `input_ids`、`output_ids`、`asset_ids`
- 通常Commit列のprimitive/schema、canonical arguments、inline payload/asset digest

`ProcedureId`、`StateId`、`JournalEventId`、branch IDそのものの一致は要求しない。strict selectorを
semantic selectorへ明示rebindしたfragmentはrebound executionとなり、解決済みbindingに対する
決定的実行だけを保証する。source state digestやraw output IDとの一致を保証しない。

別文書へのrebind transactionは全external strict selectorを明示的に置換した後、fragment全体の
portabilityとrequired preconditionを再評価する。`strict_source_only` dependencyまたは未解決strict
selectorが一つでも残れば拒否する。残らない場合だけ、利用者がexact-source保証からrebound保証への降格を
確認した上で、exporter生成の`source_document_uuid`、base `state_digest`、`id_allocation_digest` fieldを
一体で除去する。同じassertに残るcommand固有preconditionは再評価結果に従って保持または明示置換し、
無言で緩和しない。空になったassertだけを削除し、変換後fragmentを再compileしてから一回のUI model
transactionで確定する。Cancel/errorでは元fragmentを変更しない。

exporterはinvocationごとにregistryのportability evaluatorを実行し、fragment全体のclassとrequired
preconditionを出力/reportする。`strict_source_only`をportableに見せず、rebind不能理由を診断する。

## 10. clipboard fragment

### 10.1 構造

fragmentは完全fileとは別のheaderを持つ。

```inkscript
inkscript_fragment 3;

requires {
    procedure_catalog = 8;
    replay_epoch = 29;
}

program {
}
```

`requires`と`program`は必須、`parameters`、`bindings`、`assets`は必要な場合だけ含める。
`meta`、`inputs`、`output`、`execution`を含むfragmentを拒否する。file metadataはpaste先だけを正本とし、
fragmentのprovenanceや説明はcomment、step label、strict preconditionで表す。fragment単体はjobではなく、paste先の
orchestrationと結合されるまで実行できない。完全fileと同じexact-current version、型、symbol、
resource limitに従う。

### 10.2 dependency closure

fragment内の全referenceは、含まれるparameter、binding、先行step result、assetのいずれかへ一意に
解決されなければならない。選択stepが参照するparameter、binding、assetを推移的に含め、選択範囲内の
producer stepを元のprogram順で含める。未使用dependencyは含めない。

選択範囲外のmutation stepをclosureへ暗黙追加しない。範囲外resultを参照する場合は、既存stable
objectとしてstrict bindingへ変換する、copy範囲を明示的にproducerまで拡張する、診断付きで拒否する、
のいずれかだけを許可する。paste先の偶然の同名symbolへ期待して未解決referenceを残さない。

parameter、binding、stepは元source順、assetはfragment内の最初の参照順、同順位ならAssetId byte順と
する。同一AssetIdは一宣言へdeduplicateする。journal exporterがsymbol名を生成する場合はresult roleを
stemとし、出現順に`<stem>_1`、`<stem>_2`とする。hash順、localized label、summaryを使わない。

### 10.3 paste transaction

通常の挿入pasteは`inkscript_fragment`だけを受理する。`inkscript` headerを持つ完全fileは、明示的な
「スクリプト全体を置換」操作だけで受理し、parse/compile/preview成功後にsource model全体を一回の
UI transactionで置換する。完全fileをfragmentへ暗黙変換したり、`inputs`、`output`、`execution`を
既存fileへ暗黙mergeまたは破棄してはならない。通常pasteで完全fileを受け取った場合は、置換操作への
明示的な案内を示して無変更で拒否する。

pasteは実行ではない。fragment挿入は次の順に処理する。

1. fragmentを独立してparse、version検証、compileする
2. destinationのexact-current requirementと一致することを検査する
3. symbol collision、group-key collision、asset dedup、strict binding候補、挿入位置をpreviewする
4. symbol/group renameとrebindをユーザーが明示確認する
5. 全referenceを書き換えたtyped fragmentを再検証する
6. 一回のUI model transactionで挿入する

Cancel、診断、stale destinationではsource、selection、Undo、jobを変更しない。名前衝突時に既存symbolへ
無言で束縛しない。rename案は`name_2`から最小の未使用suffixを選び、宣言と全referenceを一つの
alpha-renamingとして更新する。同じAssetIdとdescriptorのassetだけを再利用し、名前またはdigestだけで
descriptor検査を省略しない。

fragment内の各`editor_group` keyは一つのgroup-local declarationとして扱う。destinationに同じkeyがある場合、
既存groupへ暗黙mergeせず、元keyへ`_2`から始まる最小の未使用decimal suffixを付け、fragment内の同groupの
全memberを同じkeyへ一括remapする。複数groupはfragmentの最初のmember出現順に処理し、remap後keyも予約して
次groupを判定する。group remapはpreview、明示確認、typed fragment再検証、単一UI transactionに含め、
Cancelまたはstale destinationで一部だけ書き換えない。

### 10.4 clipboard encoding

Windows clipboardはregistered format `Inkpod.InkScript.v3`へBOMなしUTF-8 byte列とbyte lengthを置き、
同時に`CF_UNICODETEXT`へ同じUnicode textを提供する。pasteはregistered formatを優先し、plain textは
`inkscript_fragment`または`inkscript` headerを持つ場合だけInkScript候補として扱い、10.3の操作別規則を
適用する。画像clipboardと誤認しない。

clipboard fragmentのassetはすべてinline `data`とし、`data_file`を生成しない。inline上限超過はcopyを
拒否する。textはcanonical emitterで生成し、summary、thumbnail、localized label、source pathを実行意味へ
含めない。Batch画面はfull file、group、step、step範囲、履歴画面はCommit一行または連続線形列をcopyできる。

## 11. 診断

すべてのparse/compile/bind/run errorは次を持つ。

- stable diagnostic code（例: `INKS-PARSE-0001`）
- severity
- source file/fragment identity
- 元source byte列先頭からのUTF-8 byte offsetと、1-based line / Unicode scalar columnのhalf-open
  source range。BOM、CRLF、TABは5.1の規則に従う
- 短いmessage
- field/command/selector path
- 可能な場合だけ修正hint
- item実行時はinput identity、step index/name、primitive name

秘密path、画像内容、inline asset全体をlogへ出さない。UIには必要最小限のdisplay pathを示す。
複数parse errorを回収してよいが、checksum/digest不一致やresource limit超過後に巨大dataを
走査し続けない。

## 12. resource limit と安全性

現行版は少なくとも次を上限とし、検査付き加算でtotalを計算する。既存Core側のより小さい
上限がある場合は小さい方を適用する。

| 対象                                                            |                                                       上限 |
| --------------------------------------------------------------- | ---------------------------------------------------------: |
| source text                                                     |                                                    128 MiB |
| identifier / keyword token                                      |                                                  128 bytes |
| numeric literal                                                 |                                                  128 bytes |
| tokens合計                                                      |                                                  4,194,304 |
| AST/CST nodes合計                                               |                                                  2,097,152 |
| sections                                                        |                                                          9 |
| inputs（展開前）                                                |                                                     16,384 |
| 展開後input items                                               |                                                     16,384 |
| folder列挙で検査するdirectory entry合計（match/nonmatchを含む） |                                                  1,048,576 |
| folder列挙で検査する正規化entry名のUTF-8 bytes合計              |                                                    256 MiB |
| folder列挙work units（検査entry数 + openしたdirectory数）       |                                                  1,048,576 |
| folder traversal depth                                          |                         64、またはOS adapter上限の小さい方 |
| 一native input file                                             |     exact-current native decoderのfile/section/payload上限 |
| fingerprint/readするnative／raster input bytes合計                      |                    64 GiB、またはapplication設定の小さい方 |
| parameters                                                      |                                                      4,096 |
| bindings                                                        |                                                     65,536 |
| program statements                                              |                                                     65,536 |
| nesting depth                                                   |                                                         64 |
| 一containerのfield/list element                                 |                                                     65,536 |
| list elements合計                                               |                                                  4,194,304 |
| reference path segments                                         |                                                         64 |
| dependency edges                                                |                                                  4,194,304 |
| 展開後item × enabled primitive step                             |                                                  1,048,576 |
| 一つのUTF-8 string                                              |                                                     32 KiB |
| 一つのinline decoded asset                                      |                                                     32 MiB |
| inline decoded asset合計                                        |                                                     64 MiB |
| 一つのexternal canonical asset                                  |                                                    512 MiB |
| asset logical payload合計                                       |                                                    768 MiB |
| planned logical output + temporary合計                          |                    64 GiB、またはapplication設定の小さい方 |
| 画像preview専用temporary（input＋output）                       |                                                      4 GiB |
| 画像preview RGBA8 contact sheet                                  |                                          16,777,216 pixels |
| folder naming template                                          |                                           1–1,024 UTF-8 bytes |
| diagnostics                                                     |                                                        256 |
| `wait_ms`                                                       |                                                  3,600,000 |
| aggregate wait                                                  | `wait_ms * max(planned_item_count - 1, 0) <= 3,600,000 ms` |

Base64はdecode前後の長さを先に検査する。external assetはmetadataとbounded streamで読み、
全fileを無制限allocationしない。path alias、input/output同一性、symlink/reparse point、case-fold、
volume、既存destinationをOS adapterとRust validationの両境界で確認する。

catalog entryのwork formulaをchecked加算し、static compile時のglobal bound、binding後のitem bound、
primitive直前のruntime boundの三段階で検査する。不明、overflow、unboundedを拒否する。item開始前に
worst-case Commit、StateId、ProcedureId、全stable-ID namespaceの残容量へ収まることを確認し、
不足時はID消費、mutation、temporary作成前に拒否する。output実encode中もlogical/encoded byte budgetを
enforceし、OS free-space照会だけで代用しない。

PlanTaskとRunTaskはworkspaceあたり各一件をactiveにし、application-wide bounded queueとresource
budgetを共有する。PlanTaskはfolder filter適用前に、OS adapterから観測したmatch/nonmatchすべてのentry、
正規化name bytes、traversal depth、列挙work unitをjob/application counterへchecked加算する。超過時は列挙を
cancelしてplan全体を無変更で拒否し、先頭16,384件だけへ暗黙truncateしない。fingerprint hash/read bytes、
RunTaskのnative／raster read bytes、timer continuationの残wait budgetも同じcounterへchecked加算する。上限到達後の
parser recovery、Base64 scan、diagnostic生成も残りbudget内に制限する。

scriptは権限境界を拡張しない。path intentをauthority前に表示し、absolute input、script directory外の
asset、output root、replaceを別capabilityとして確認する。network URL、UNC、暗黙download、shell
expansionを行わない。

parser、Base64、descriptor、catalog decoder、selector、fragment dependency collectorは
malformed testとfuzz targetを持つ。allocation failure、cancel、stale input、output collision、
queue saturation、shutdown raceをfault injectionで検証する。

## 13. 完全な例

次は現行file v3／catalog v8のsyntaxと、`replace_raster_colors`、`resize_document`の規範的なfieldを示す。
四種類に限定された製品Batchの作成例ではなく、公開Rust runtime用の一般script例である。
他commandのfieldをこの例から類推して追加してはならず、procedure catalogのexact signatureに従う。

```inkscript
inkscript 3;

requires {
    procedure_catalog = 8;
    replay_epoch = 29;
}

meta {
    name = "Color cleanup";
    description = "Replace one approved color and resize each cell.";
}

inputs {
    folder "input" {
        cells = range(1, 120);
        recursive = false;
    };
}

parameters {
    param replacement: pixel_value = rgba8(20, 80, 255, 255) {
        label = "Replacement color";
        ask = each_run;
    };
}

bindings {
    let paint = select plane {
        plane_kind = color;
        cardinality = one;
        missing = error;
    };
}

program {
    step "Replace red" {
        enabled = true;
        invoke replace_raster_colors {
            plane_id = $paint;
            pairs = [
                {
                    enabled = true;
                    old = rgba8(255, 0, 0, 255);
                    new = $replacement;
                },
            ];
        };
    }

    step "Resize document" {
        enabled = true;
        invoke resize_document {
            resize = {
                width = 1920;
                height = 1080;
                dpi_x_milli = 144000;
                dpi_y_milli = 144000;
                anchor = center;
                resample = true;
            };
        };
    }
}

output {
    policy = duplicate;
    format = inkpod;
    folder = "output";
    cell_folder = false;
    basename = "painted";
    start_number = 1;
    direction = ascending;
}

execution {
    failure = continue;
    wait_ms = 0;
    preview_before_save = true;
}
```

## 14. 実装配置と公開境界

### 14.1 Rust

- `inkpod-format`
  - `inkscript/source`: UTF-8/BOM/line map/source span
  - `inkscript/lexer`: bounded tokenization
  - `inkscript/parser`: Core非依存lossless CSTとrecovery付きrecursive-descent parser。局所source editはM6で追加する
  - `inkscript/syntax`: Core非依存semantic syntax AST
  - `inkscript/types`／`names`／`fragment`: typed value、namespace、dependency closure
  - `inkscript/envelope`: input／parameter／output／executionのtyped model
  - `inkscript/emit`: typed canonical emitter / fragment writer
  - `inkscript/diagnostic`: stable diagnostic code
- `inkpod-core`
  - `script/catalog`: stable command/assert/selector schema
  - `script/compile`: syntax ASTからtyped static programとPathIntentPreviewへの変換
  - `script/plan`: cancellable input/asset/output planningとconfirmation token
  - `script/bind`: initial document selector解決
  - `script/assets`: bounded asset ingestion
  - `script/execute`: common primitive executorへの接続
  - `script/run`: sequential staged runner、dry-run、itemごとのfile／staged result
  - `script/output`: shared codec、materialize、active／new-tabのpublication所有権
  - `script/preview`: 専用temporaryのcopy／save-reopenとcontact sheet
  - `script/io`: 共有Rust I/O managerを使うCore-only authority／plan／run adapter
  - `script/export`: canonical journalからfragment ASTへの変換
  - `script/report`: preview/dry-run/run report
- `inkpod-ffi`
  - opaque script source/plan/preview/report/export handle
  - source bytes、diagnostics、summary、rowをbatch/span単位でcopyするAPI

`inkpod-format` は `inkpod-core` に依存しない。syntax ASTに `LayerId`、`PixelValue`、
`CanonicalInvocation` を入れず、Core compile境界でtyped valueへ変換する。公開Rust APIは
C ABI recordに依存しない。

### 14.2 Windows

以下はM5–M15で接続する設計境界とする。現行製品Batch paneは切替まで既存modelを使う。

- `ScriptController`は`WorkspaceWindow`のBatch pane単位でsource/plan/task lifecycleを所有する。
- 切替後のBatch paneはsource ASTのprojectionとし、独自Batch modelを第二の正本にしない。
- file picker、authority取得のUI、registered clipboard format、rebind dialogはC++が担当する。
  file identity・排他・原子的置換は共有Rust I/O managerと`inkpod-io`のprivate platform backendへ委譲する。
  既存Windows private authority adapterからの移行でも8節のauthority／race契約を弱めない。
- command発行時のimmutable `CommandContext`へworkspace/session/view/pane/job IDとgenerationを固定し、
  state queryとexecutionで同じtarget解決を使う。
- Core非依存のUTF-8 lex/parseはbounded worker taskで行ってよい。typed compile、snapshot取得、bind、
  execute、export、Core handle registry操作はCore engine threadで行う。
- UI threadはCoreやPresentを同期的に待たず、workspaceごとのstatus bar共通progress／cancelへ接続する。
- Rendererはpreview用immutable snapshot以外のscript stateを所有しない。

### 14.3 C ABI

- source textはUTF-8 pointer + byte lengthとして呼出中だけborrowし、Rustが必要量をcopyする。
- diagnosticsとscript textは二段階caller buffer APIで取得する。
- plan/report/export handleはRust allocationで、対応releaseを必須とする。
- path、parameter、fragmentの可変長入力はbounded bulk APIで取り込む。
- raw AST node pointer、Rust enum、`Vec`、`String`、C++ STLをABIへ出さない。
- panic/exceptionはABIを越えない。NULL、alignment、size、enum、count、overflowをnegative testする。
- 各opaque handleはowner controller、owner thread、immutable cross-thread可否、parent lifetime、release
  thread、session generation invalidationをheaderと`docs/ffi.md`へ規定する。stale handleを現在の別sessionへ
  再解決しない。
- public symbolまたはrecordを追加・削除・変更するmilestoneはC ABI versionを同じ変更で更新し、
  header/Rust drift、C11/C++20 include、旧version拒否smokeを更新する。

## 15. `.inkbatch` 廃止方針

M15の明示cutoverまでは、`.inkbatch` v5と既存Batch pane／ABIを唯一のBatch production routeとして維持する。
InkScriptはprivate harnessで同等性を検証し、二つのuser-facing正本を恒久運用しない。

production cutover milestoneでは、Batch pane、open/save filter、resource、clipboardのuser-facing正本を
`.inkscript`へ一本化し、同じ変更で`SPEC.md`、README、file format、architecture、FFI、compatibility、
関連する追跡表を更新する。現在状態と代表evidenceは`docs/compatibility.md`だけへ記録する。
`BATCH-*`の過去evidenceと許可された状態値は残し、対応欄に
`Superseded by SCRIPT-*`と記録する。`Superseded`を互換状態値として追加しない。同じcutoverで公開
`.inkbatch` open/save/run ABI symbolとfile filterを削除し、C ABI version、header、FFI docs、旧version
拒否testを更新する。旧Batch実装は次milestoneまでtest-private shadow comparatorとしてだけ残せる。

後続の削除milestoneでは、test-privateになった`.inkbatch` parser/writer、BatchGraph persistence、旧runner、
fixtureを削除する。公開symbol/filterの廃止やABI更新をここまで遅延させない。共通algorithmとcanonical
domain typeは適切なRust ownerへ移し、production codeからtest-private legacy ownerへの参照を禁止する。
現行`.inkpod`が保持する`ApplyBatchOperations`のpayload、replay decoder、Undo/Redo、fill protection、
fragment exportは旧ファイル形式の実装ではないため削除しない。canonical replay契約を変える場合は別の版更新判断を要する。
production sourceから旧reader/APIを除去しても、compatibilityの履歴、廃止理由、`.inkbatch` extension/magicを
安全にunsupportedとして拒否するnegative testは残す。docs/tests内の文字列を無差別に全消去しない。

`.inkbatch` reader、one-shot importer、自動migration、互換shimは残さない。

既存 `.inkbatch` の移行機能が必要になった場合は、フォーマットフリーズ前の現行versionのみ
方針に対するユーザーの明示的な別指示を必要とする。この文書だけを根拠に追加してはならない。

## 16. マイルストーン運用規則

本節から18節までは、ユーザーが実装の再開を明示したときに適用する。計画の改訂依頼だけでは
実装を開始しない。再開時はM1から着手し、M15の製品切替までは現行Batch v5を維持する。
一工程の再開は、未決の製品契約、性能基準、cutoverの承認を兼ねない。

### 16.1 状態と受入

- `[ ]`: 未着手、または完了条件を満たしていない。
- `[~]`: 実装と必要な自動検証、または判断資料の準備が完了し、利用者確認・明示判断を待つ。
- `[x]`: 必要な確認・判断を含め完了条件を満たした。
- `[!]`: 受入で問題が見つかり、同じ工程の修正が必要。

一sessionでは指定された一工程を完了条件まで進め、後続の実装へ進まず報告する。
未指定なら先頭の未完了工程を扱う。`[!]`は修正を優先し、`[~]`は利用者からの確認結果を反映する。
汎用の再開prompt、無回答、文書改訂を承認済みと読み替えない。既に得た承認は再確認しない。
依存する判断が未決ならその変更だけを止め、独立した調査・検証は進める。

markerは実装計画内の受入管理に限る。要件の状態、既知差分、代表検証は
[compatibility](docs/compatibility.md)に記録し、ここへ過去の実行ログや全sampleを複製しない。
過去の完了はGit履歴から確認する。番号の並べ替えで未完了gateを完了扱いにしない。

### 16.2 各工程の共通完了条件

1. `git status`、既存差分、関連するSPEC／code／testを確認し、依頼外の変更を保護する。
2. 挙動変更は公開契約のtestを先に固定する。既存契約との不一致は、依頼された変更、既存不具合、
   未決仕様に分類する。正本の優先順位だけを理由に期待値を変更しない。
3. success、no-op、invalid、cancel、stale、overflow、allocation failure、resource上限の該当経路を検証する。
   文書変更ではcanonical procedure、Undo/Redo、ID、dirty／revision、必要なsave/reopenとcache-free replayを確認する。
4. [verification](docs/verification.md)の変更種別に従いRust、ABI、Windows、可視経路を検証する。
   private smoke、製品UI、実機の証拠を区別し、未実施の検証を完了扱いにしない。
5. source／plan／task／report／snapshotの所有権、owner thread、generation、cancel／close／releaseを確認する。
   UIはCoreやrendererを同期的に待たず、失敗時に別sessionへ再解決しない。
6. file／catalog／replay epoch／native top-level／C ABIへの影響を判定する。serialized schemaやreplayを
   変更するときは[版更新規則](#1-文書の位置付け)とSPECに従い、現行値、registry、旧版拒否、生成物を同時更新する。
   新版番号を計画時点で予約せず、実装時のexact-currentから決める。
7. workload、harness、意味counter、環境別envelope、canonical `revision-max`式を変更する場合は、
   理由・比較・全sampleを用意して明示承認を得る。失敗を削除、ignore、tolerance緩和で隠さない。
8. 必要な縦切りを統合し、未接続UIやstubを完成扱いにしない。製品接続前の工程はprivate harnessまたは
   明示されたCore-only／ABI scopeとして検証する。分担時は編集ownerと統合担当を固定する。
9. 最終報告は利用者向け挙動、設計判断、変更file、version impact、検証結果、未検証事項、必要な確認手順を示す。
   code/buildを変更した工程は原則`[~]`で受入を待ち、手動確認不要ならその根拠を示す。
10. commit、push、PR、外部公開は別途明示依頼された場合だけ行う。

### 16.3 再利用する基盤と再開時の確認

現行versionの正本は[SPEC 20節](SPEC.md#20-形式白透過一般画像入出力)と1節のregistry参照先である。
再開時に宣言値と照合する。以下は再実装する工程ではなく、変更の影響を検証する入口である。

| 基盤 | code／代表test | 再開後の扱い |
| --- | --- | --- |
| UTF-8、lossless CST、typed AST、emitter、fragment closure | [format InkScript](rust/inkpod-format/src/inkscript/mod.rs)、[parser tests](rust/inkpod-format/tests/inkscript_parser.rs)、[program tests](rust/inkpod-format/tests/inkscript_program.rs) | 継続利用。source局所編集と承認されたenvelope拡張だけを追加する |
| 75-command compile／bind／executeとcanonical exporter | [Core script](rust/inkpod-core/src/script/mod.rs)、[public contracts](rust/inkpod-core/tests/inkscript_public.rs)、[registry tests](rust/inkpod-core/tests/inkscript_registry.rs) | owner全単射とexact-source／rebound保証を維持。M3 の四処理 command／export を含む |
| authority／PlanTask／RunTask／report | [plan](rust/inkpod-core/src/script/plan.rs)、[run](rust/inkpod-core/src/script/run.rs) | 現行native経路から製品I/O・preview・staged publicationへ接続する |
| source／export／execution C ABI | [FFI source](rust/inkpod-ffi/src/inkscript.rs)、[FFI execution](rust/inkpod-ffi/src/inkscript_execution.rs)、[ABI tests](rust/inkpod-ffi/tests/unit/inkscript.rs) | 既存handleと失敗契約を再利用し、不足する境界だけを追加する |
| Windows private authority／engine route | [authority tests](tests/windows_inkscript_file_authority.cpp)、[engine tests](tests/windows_inkscript_engine_route.cpp) | production公開済みとは扱わず、共有I/Oとstatus barへの接続を検証する |
| 現行Batch四処理・I/O・preview | [Batch v5 tests](rust/inkpod-core/tests/contracts/batch_v5.rs)、[Batch contracts](rust/inkpod-core/tests/contracts/batch.rs)、[FFI Batch tests](rust/inkpod-ffi/tests/unit/batch.rs) | M9の比較元。既存algorithmとcanonical executorを再利用する |
| InkScript quick benchmark | [runner](rust/inkpod-core/src/script/performance.rs)、[承認済み基準](docs/core-benchmark-baseline.md#approved-inkscript-quick-envelope) | Release専用。既知のnative-byte期待値不一致をM2で扱い、Debug test成功で代用しない |

full benchmarkは[予約済みfixture](docs/core-benchmark-baseline.md#reserved-inkscript-full-fixture)であり、
現行版で実装・成立したgateではない。M17で再照合する。性能失敗やWindowsの間欠失敗は
compatibilityの該当する既知差分を保持し、今回の計画改訂で解決済みとしない。

### 16.4 現行Batchとのparity

比較元は現行SPEC 19節とテスト済みの`.inkbatch` v5である。旧Batchのseed／filter／native separation
作成UIを復元する計画ではない。`.inkbatch`を読み込んで新形式へ移行する製品機能も作らない。

| 要件 | 新しいowner工程 | 必須evidence |
| --- | --- | --- |
| `BATCH-001`、`SCRIPT-001` | M1、M4–M9 | 固定Input／Output、順序・enable・複製・削除、set保存／再読込、lossless sourceとのprojection、draftと実行用immutable planの分離 |
| `BATCH-002`、`SCRIPT-002` | M1、M3、M7、M9 | 色置換／彩色プレーンへ送る／マスキング／消去、四処理に閉じたauthoring、全enabled処理列の一transactionとcanonical invocation一致 |
| `BATCH-003`、`SCRIPT-003` | M4、M5、M8、M9 | file／folder／発行時active入力、各codec、folder／active／新規tab出力、命名・衝突、dry-run、画像preview、progress／cancel、item atomicity、staged所有権 |
| `BATCH-004`、`SCRIPT-002`、`SCRIPT-004` | M3、M7、M9–M12 | Color／Raster roleとfixed ID、target重複除去、MainLine拒否、native-depth exact pair・ambiguity、移動とfill protection置換、export／paste／rebind |
| `SCRIPT-005` | M2、M9、M13–M18 | private parity、fuzz／fault／path race、明示cutover、旧形式削除、quick／full性能と製品hardening |

M9はinput順、resolved target、output plan、全canonical procedure列とCommit境界、state／composite digest、
history／Undo／Redo、全namespaceのID high-watermark、fill protection、document/editor savepoint、dirty／revision、
report、意味work counter、save/reopen、cache-free replay、failure atomicityを比較する。
active／新規tab／previewは各publication境界のidentity・path authority・元targetも比較する。
source UUIDが異なる新規tab同士は新identityの生成条件を検証し、reboundとexact-sourceの保証を混同しない。

### 16.5 旧番号と新計画の対応

他文書の旧gate名は次の工程として読む。gateの受入条件は採番し直しても解除しない。
この対応表は参照用であり、旧番号を新しい実装順序として使わない。

| 旧工程 | 新工程 |
| --- | --- |
| M00–M27Bの完了済み基盤 | 16.3節の再利用対象。新たな実装工程には数えない |
| 中断後のBatch／I/O／性能差分 | M1–M5として追加 |
| M28A／M28B | M6／M7 |
| M29A／M29B | M8、および現行exact-pair作成を扱うM7。旧advanced Batch UIの復元は含めない |
| M29C shadow parity | M9 |
| M30／M31／M32 | M10／M11／M12 |
| M33A／M33B | M13／M14 |
| M34 明示production cutover | M15 |
| M35 旧形式の完全削除 | M16 |
| M36 full performance | M17 |
| M37 最終hardening | M18 |

registryの`owner_milestone`、equivalence ID、既存test名に含まれるM07、M08、M15等は安定した
識別子であり、この新しい工程番号とは別に保持する。owner manifest内の既存IDを改番しない。
将来のschema変更時に追跡表を更新する場合も、この対応と製品gateを明示する。

## 17. 実装マイルストーン

M1–M5で現行契約との不足を解消し、M6–M14はprivate経路を完成させる。
M15だけが製品切替であり、M16–M18までを完了範囲とする。実装順序は番号順を基本とし、
一工程内の独立調査・レビュー・検証は並列化する。

### [x] M1 — 現行Batchへの接続契約と版更新方針の確定

**利用者判断完了**：2026-09-06の「OK. 推奨案で実装してください。」により
[接続契約](docs/inkscript-batch-connection.md) D1–D4の推奨案が承認された。
source、typed command／target、input profile、output／preview、UI projection、版更新と後続testの
実装契約とする。現行v2／v7の受理範囲と製品経路はまだ変更していない。
性能基準変更・production cutover・旧形式削除はこの承認に含めない。代表検証と既知差分は
[compatibility](docs/compatibility.md) を参照する。

**範囲**

- 1.1節と16.4節の差分をcode／public testで照合し、source例、入出力と失敗例、UI projection、
  所有権、canonical procedure／Undo境界を具体化する。作業ログの再収集ではなく実装可能な契約を作る。
- 推奨案は、四種類のclosed operation variantを含む順序付き処理列を一つのtyped commandで表し、
  既存`ApplyBatchOperations`へ一回渡す方式とする。command名・field・target binding・resource bound・
  export／rebindの保証を確定する。別案では既存parityを満たせる根拠を示す。
- raster入出力、bounded命名、active／新規tab出力、画像preview、source編集の公開導線と四処理paneの関係を
  確定する。存続する一般script、`current_sequence`、each-run parameterを製品のどこへ公開するかを区別し、
  現行paneへRun current／独立dry-run／廃止機能を自動追加しない。
- grammar／catalog／bindingの変更、native／replay／ABIへの影響を列挙する。v2／v7の受理範囲を
  この工程で実装変更せず、M3–M5で使う承認対象をレビュー可能な形にする。

**完了条件**

- 未決の外部観測可能な挙動について根拠・選択肢・推奨案・影響が揃い、必要なユーザー判断を得ている。
- 仕様、registry、test、各実装ownerへの変更一覧と、四処理の一transactionを含むparity条件が確定している。
- 現行Batchは動作を維持し、`.inkscript`公開、旧形式削除、性能基準変更は承認に含めていない。

### [x] M2 — Release quick性能gateの現行版整合

**利用者判断・検証完了**：private診断経路、5 literal更新、現行環境への同じ64–107 ms範囲の適用は
それぞれ明示承認を得た。[承認済み基準](docs/core-benchmark-baseline.md#m2-approved-current-version-correction)に
固定fixtureからの導出、元の失敗、全counter、診断sampleと独立した元gateの全sampleを保持する。
元gateはwarm-up後9回すべて成功し、中央値86.8725 ms。実装はtest-onlyの診断と承認済み期待値更新で、
製品経路・画素・形式・ABIを変更しないため追加の手動UI受入は不要。代表検証はcompatibilityに置く。
full性能は未実装のままM17に残し、この作業ではM3へ進まない。

**範囲**

- compatibility記録のnative入力長期待値不一致を現行版で再現し、byte／digest依存の差と
  workload／非byte意味counterの差を分ける。元の失敗を保持する。
- 最小の是正案、固定fixtureからの導出、同条件の全sampleを提示し、期待値・harness等を変更する前に
  必要な明示承認を得る。画質、処理数、cancel／failure、cache-free replayの検査を減らさない。

**完了条件**

- Release専用quick gateが現行versionで独立して成立し、counterとchecksumの根拠が揃う。
- 承認済みx64 envelopeは該当環境で検証し、異なる環境の結果で代用しない。
- 以後の版変更でも同じgateを維持する手順が明確で、full未実装をquick成功で完了扱いにしていない。

### [x] M3 — 四処理のcanonical catalog接続とfragment export

**完了**：四処理の Core／既存 ABI scope と fragment export を実装・検証した。
catalog v8／75 command を使用し、file v2／epoch 29／native v34／ABI v34 は維持する。
checksum 一 literal の更新は明示承認を得て適用し、[元の Release quick gate](docs/core-benchmark-baseline.md#m3-catalog-v8-checksum-decision)
も独立検証で成立した。16.2 節の明示 Core／既存 ABI scope として公開契約を検証し、新しい製品 UI 操作を
含まないため追加の手動 UI 受入は不要と判断する。既存製品 Batch の英日可視経路の検証とは区別する。
後続 M4、製品 cutover、full 性能 gate へは進まない。代表検証と未検証範囲は
[compatibility](docs/compatibility.md) に記載する。

**範囲**

- M1で確定したclosed command／operation list／target bindingをformat、registry、Core compilerへ実装する。
  処理順とenableを保持し、全enabled処理とtargetを一つの既存canonical executorへ渡す。
- Color／Raster role、fixed ID、色置換の全layerへの決定的展開・重複除去、MainLine拒否、native-depth exact color、
  atomicな移動、fill protection置換、消去を既存Rust処理で実行する。
  色置換以外の処理もそれぞれの現行target解決契約を保持し、全layer処理へ暗黙拡張しない。
- exporterのprivate Batch拒否から承認済み表現への対応を追加し、strict selector、list／order、
  dependency closure、portability、exact-source／reboundを検証する。生成referenceとowner全単射も更新する。

**完了条件**

- Core public APIから四処理と組合せのdirect／script／export再実行を比較し、pixelだけでなく
  canonical invocation、一Commit／Undo、ID、mask、save/reopenとcache-free replayが一致する。
- no-op、全無効、missing／hidden／non-editable、形式不一致、重複、cancel／overflowで部分commitしない。
- 必要なfile／catalog／native／replay更新と旧版拒否が同じ変更で揃い、製品UIへはまだ接続しない。

### [!] M4 — 製品入出力・画像previewのRust実行経路

**未完了（修正が必要）**：承認済み D2/D3 の Core-only 入出力・staged result・画像preview を実装した。
file／fragment v3、catalog v8／75 command、epoch 29／native v34／ABI v34 を使用する。
共有 Windows I/O の guarded overwrite は、外部の write／rename を禁止した source handle と
原子的置換の両立が未解決で、成功契約の test が失敗している。保護を外す fallback は追加せず、
この工程の完了条件を満たしたとは扱わない。file v3 による Release quick の checksum 一値更新は
明示承認を得て適用し、[元の gate の独立検証と全sample](docs/core-benchmark-baseline.md#m4-file-v3-checksum-decision)
を記録した。性能基準は維持し、上書き処理の未解決事項と M4 の状態は変えない。
代表検証・既知差分は [compatibility](docs/compatibility.md) に記録する。
次回は M4 の修正と検証だけを行い、M5・製品 cutover へ進まない。

**範囲**

- M1で承認した入力／出力envelope、typed plan／runner／reportを実装する。共通codecと共有I/O managerを
  使用し、file／非再帰folder／発行時active入力、folder／active／新規tabのstaged結果を扱う。
- bounded template、自然順・重複・衝突、MainLine保護、maskを保持できないraster出力の拒否、
  capacity preflight、active出力の一Undoとsavepoint不変、新規tabの新identity／pathless／dirtyを検証する。
- 画像previewは全入力をcopy／materializeしてから実行し、出力形式で保存再読込してcontact sheetを作る。
  SPECのtemporary容量／pixel上限、cleanup-before-publication、clean/pathless表示専用結果を維持する。
  authority preview、staged dry-run、画像previewを異なる結果型・副作用として扱う。

**完了条件**

- Core-onlyの公開契約から各入力／出力、dry-run、preview、cancel／failure／staleを検証できる。
- 共通I/Oのauthority、lock、同volume atomic install、失敗item非公開、成功済み先行item保持が成立する。
- previewは実outputとlive sourceを変更せず、元target contextを保持し、cleanup失敗では公開結果を返さない。

### [ ] M5 — ABIとWindows engine／共有I/O adapterの統合

**範囲**

- M3–M4のsource／plan／report／staged resultを必要なbounded C ABIへ接続する。
  opaque handle、二段階copy、take／release、immutable cross-thread DTOとowner threadを固定する。
- 既存private InkScript engineを拡張し、file identity・排他・原子的置換を共有Rust managerへ委譲する。
  既存Windows private authority adapterの最終identity検証とno-lost-update／temporary guardを移管先でも満たす。
- active／新規tab／previewのpublicationを発行時contextへ束縛し、status bar共通progress／cancelへ接続する。
  pane登録、製品file filter、公開commandの切替は行わない。

**完了条件**

- ABI version／header／export／C11・C++20／negative testsが整合し、追加境界のownershipを文書化する。
- private実Windows経路で各codec、staged take／release、close／shutdown／queue saturation、cancelとsave failureを検証する。
- C++にcodec／selector／画像処理や第二のI/O engineを残さず、staleな別sessionへ通知・公開しない。

### [ ] M6 — private source controllerとfile lifecycle

**範囲**

- 製品paneに登録しない`ScriptController`／private harnessへsource、diagnostics、plan／taskのlifecycleを接続する。
- lossless CSTの局所編集、UTF-8／UTF-16位置対応、new／open／save-as／dirty／atomic saveを実装する。
  編集用sourceのUndoとdocumentのUndoを混同しない。

**完了条件**

- comment、BOM／CRLF、無関係rangeを保持して編集・保存・再読込できる。
- 文法エラーは診断と修正可能なdraftとして保持し、実行可能planを返さない。操作拒否・取消・save失敗では
  意図しないsource置換、dirty解除、部分file保存を起こさない。
- source世代変更で旧plan／confirmationを失効させ、製品Batchの正本は既存model一つのままである。

### [ ] M7 — 四処理paneとsourceの相互編集

**範囲**

- M6 harnessで固定Input／Output、四種類の処理、順序・enable・複製・削除とtyped parameterをsourceへ反映する。
  複数setは保存fileの集合として管理し、旧計画の複数setを一sourceへ詰める未定義構文を導入しない。
- 現行のset名dropdown、工程checkbox、inline scrollable parameter、exact-depth色編集、target roleと
  読込済みfixed ID保持を再現する。二枚の固定sourceによるcolor pair抽出、bounds／件数／alpha表示、
  one-to-manyの明示選択または除外、many-to-oneを既存Core queryへ接続する。
- M1で決めたsource／一般scriptの編集導線を接続し、四処理で表現できないstepを損失変換しない。
  `editor_group`を処理列のtransaction境界として使用しない。

**完了条件**

- UI→source→save/reopen→UIのround-tripで処理列・selector・型・commentが保たれる。
- pair抽出のdimension／format／identity不一致、stale、ambiguity、Cancelでsourceやjobを部分変更しない。
- 日本語／英語、狭いpane、keyboard、geometry-only resizeをprivate可視経路で確認する。
  seed／filter／separation等を四処理の追加menuへ復元しない。

### [ ] M8 — private preview／全実行／中止UI

**範囲**

- M7 harnessをM5 engineへ接続し、input／output validation、画像preview、全実行、中止、
  failure report、必要なauthority／confirmationとeach-run parameterを扱う。
- 最下段は現行の三buttonとし、独立dry-runやRun currentの追加はM1で別途確定した導線に限る。
  result欄のlocalized item理由・先頭8件と残件数、copy／scroll、共通status barを維持する。

**完了条件**

- private可視UIからfolder／active／新規tab出力とcontact-sheet previewが動く。
- preview tabからの次jobも元の発行時targetへ固定し、staleなら拒否する。
- continue／stop、wait、confirmation失効、close、cancel、copy／encode／cleanup／save失敗が
  reportと実際の公開結果に一致し、source／default／既存destinationを不当に変更しない。

### [ ] M9 — 現行Batch v5とのshadow parity gate

**範囲**

- 同じfixtureを現行Batch v5とprivate InkScriptへ与え、16.4節の全項目を比較する。
  fixture変換はtest専用とし、production importerを作らない。
- 四処理／組合せのRGBA8/16 target、scalarを含むexact-pair query、MainLine／未対応targetの拒否、
  色置換の複数target、fill protection、各入力／出力、previewと
  failure policyを覆う。日本語／英語、keyboard、DPI／high contrast／accessibilityも確認する。

**完了条件**

- `BATCH-001..004`と`SCRIPT-001..005`への追跡表が現行契約を覆い、成功・no-op・失敗のparityが成立する。
- private x64 Release／可視UIの必要な証拠があり、未検証の実機項目を成功と数えない。
- 旧M29Cのgateを満たしても製品file／commandは切り替えず、M10–M14とM15の明示承認を待つ。

### [ ] M10 — Batch fragment clipboard

**範囲**

- private Batchのstep／range／group copyとfragment挿入paste、明示的な全script置換を接続する。
  registered formatと`CF_UNICODETEXT`は実装時のexact-current版に揃える。
- dependency closure、asset dedup、name collision、parameter／binding書換え、bounded clipboard所有権を扱う。

**完了条件**

- Batch→Batch、text editor→Batchが同じcanonical textでround-tripする。
- full fileを通常pasteで暗黙merge／破棄せず、oversize／range外producer／invalid／Cancelでsourceとjobを保持する。
- pasteは一回のsource編集transactionとし、document編集のCommit／Undo単位を変えない。

### [ ] M11 — History fragment clipboard

**範囲**

- private History導線から、一Commit／線形列を既存canonical exporterとM3のBatch対応でcopyする。
  active／inactive branch、typed result、asset closure、snapshot authorityを扱う。
- 表示summary、thumbnail、localized labelを実行textへ流用しない。

**完了条件**

- History→Batchでexact parent assertions、処理列の一Commit境界、result／ID／assetが保たれる。
- 非連続／非線形／非Commit、close race、oversize、cancelを診断し、source documentを変更しない。
- registered formatとUnicode fallbackが同じcanonical fragmentを返す。

### [ ] M12 — strict bindingの明示rebind

**範囲**

- kind／name／owner／formatに基づく候補を表示し、外部strict selectorをsemantic selectorへ明示置換する。
  saved mask、Color／Raster role、四処理のtarget listも含める。
- 全置換後にportabilityとpreconditionを再評価し、利用者確認の上でexact-sourceのUUID／state／ID allocation
  assertionを除去してrecompileする。`strict_source_only`は理由を表示して拒否する。

**完了条件**

- History→別document、Batch→Batch、list result／assetを含むケースがrebound保証どおりに動く。
- 0件／複数候補、stale、strict残存、Cancelで無変更とし、active objectや同名objectへ暗黙再解決しない。
- exact-sourceから保証が変わることを表示し、paste／rebindは一回のsource編集transactionで確定する。

### [ ] M13 — malformed-inputとfuzz regression

**範囲**

- 既存lexer／parser corpusを拡張し、emitter、catalog、四処理list、selector、asset、envelope、
  clipboard、結果reportの境界を検査する。lossless edit後の不正sourceも含める。

**完了条件**

- panic、unbounded recovery、resource上限回避を防ぎ、allocation失敗を規定のerrorとして扱う。
- crashは最小化したregressionへ固定し、旧版／未知field／malformed inputを明示拒否する。
- fuzz smokeと変更範囲の公開契約testが成功し、既存negative caseを削除しない。

### [ ] M14 — fault injectionとpath race hardening

**範囲**

- allocation failure、queue saturation、close／shutdown、confirmation再利用、file replacement、
  reparse／alias／path raceを各staging・publication・install barrierへ注入する。
- preview copy／cleanup、active／新規tab publication、shared directory graphも対象にする。

**完了条件**

- partial install、resource leak、別fileのcleanup、別sessionへの適用がなく、reportとdestinationが一致する。
- install後cancelは成功済みitemを取り消さず、install前cancel／staleは公開しない。
- 対象Rust／ABI／Windowsとquick gateが成功し、未解決の間欠失敗を再実行成功だけで隠さない。

### [ ] M15 — 明示production cutoverと公開契約の一本化

**開始条件**

- M1–M14の必要な受入が完了している。切替差分、旧形式拒否、version impact、検証結果と
  利用者が確認できるprivate経路を提示し、製品切替そのものの明示承認を得る。

**範囲**

- 通常Batch paneのowner／model／commandを完成済み`ScriptController`へ一回で置換する。
  Batch専用tab、四処理UI、status bar、set保存、file filter、clipboardを新しい正本へ接続する。
- 公開`.inkbatch` open／save／run ABI、export、file filterを同時削除し、必要なC ABI更新、header、
  FFI docs、旧版拒否を揃える。旧実装は次工程までtest-private comparatorに限って残す。
- SPEC、README、architecture、file-format、FFI、追跡表、compatibilityの該当箇所を同じ変更で更新する。
  M9の結果から旧runnerへ依存しないcanonical／state／ID／report goldenを固定する。

**完了条件**

- 通常UIからM9–M14の該当parity／clipboard／rebind／fault契約を再検証し、利用者受入を得る。
- `.inkbatch` extension／magic、旧ABI、削除symbolを拒否し、二つの製品正本やmigration／shimを残さない。
- `BATCH-*`の証拠と後継`SCRIPT-*`の対応を保持する。M17–M18を含む残件がある要件は一括でVerifiedにしない。

### [ ] M16 — 旧`.inkbatch`形式・runnerの削除

**範囲**

- test-privateの旧reader／writer、BatchGraph persistence、旧runner／UI model、専用fixtureを削除する。
  再利用するalgorithm／domain typeのowner移動と削除は検証可能な工程に分ける。
- 現行native journalの`ApplyBatchOperations`payloadとreplay、四処理domain type、fill protection、
  共通codec／pair抽出／contact sheetを保持し、旧形式の所有権から切り離す。

**完了条件**

- productionから旧形式ownerへの参照がなく、M15のgoldenが旧comparatorなしで通る。
- native save/reopen／cache-free replay／Undo／Redoとscript exportが継続し、旧形式だけを安全に拒否する。
- public symbol／filter削除をこの工程へ先送りせず、履歴・廃止理由・negative testの文字列は保持する。

### [ ] M17 — full性能fixtureの現行化とgate接続

**範囲**

- 予約済みfull fixtureを現行contractへ照合し、旧版依存bytes／checksumと不変の意味counterを分ける。
  旧M36の予定を現行版の承認済み測定結果として扱わず、必要な再基準化は理由・全sample・counterを示して承認を得る。
- 1,024 step、large asset、multi-item、cancel／save failure、cache-free replayの固定workloadを実装し、
  quickと既存Core性能gateを併せて検証する。無断でworkload／harness／envelopeを緩和しない。

**完了条件**

- 同一環境のwarm-up後全sample、中央値、checksum、counter、測定区間が基準文書へ記録される。
- 該当envelopeで成立し、独立再測定でも残る回帰を完了扱いにしない。別環境は別証拠とする。
- fullの実装・版整合・性能判定が揃い、Debug／quickだけで代替していない。

### [ ] M18 — 製品hardeningと最終受入

**範囲**

- 日本語／英語、IME、DPI、high contrast、screen reader、Tab／F6、狭いpaneとresize、
  長時間job／反復soak、device reset中progress、close／shutdown／clipboardを製品経路で検証する。
- verificationの該当Rust／Windows構成、ABI、fuzz regression、quick／full性能gateを完了する。

**完了条件**

- UI→Core→保存／再読込の縦切りとownershipが成立し、未解決failureがない。
- CI、非表示native、可視経路、実機・platform／configuration別の証拠と未検証事項が区別される。
- 利用者受入と各要件の完了条件を満たした範囲だけcompatibilityを更新し、残る制約を明記する。

## 18. 再開プロンプト例

計画改訂後、実装を始めるときに使用する。最初の対象はM1である。

```text
INKSCRIPT.mdの実装作業を再開してください。AGENTS.md、SPEC.mdの関連要件、git status、既存差分、
対象code/test、docs/compatibility.mdの該当行を確認し、先頭の未完了マイルストーンを一つ進めてください。
確認待ちの工程があれば、このメッセージに明記した確認結果だけを反映してください。
この再開依頼だけで未決の製品契約、性能基準変更、production cutoverを承認済みと扱わないでください。

現在のBatch v5の四処理、全処理列の一canonical transaction／Undo、I/O・preview・publication契約を
維持し、計画された依存順に進めてください。既存のlanguage／Core／ABI基盤を再利用し、
実装範囲外のrefactorや廃止機能の復元は行わないでください。

判断が必要なら根拠、具体案、選択肢、推奨、影響を準備し、依存する変更だけを止めてください。
公開契約をtestで固定し、INKSCRIPT.md 16.2節とdocs/verification.mdに従って検証してください。
一工程の完了後は後続を実装せず、挙動、設計判断、変更file、version impact、検証結果、
未検証事項と必要な利用者確認手順を報告してください。commit、push、PRは行わないでください。
```
