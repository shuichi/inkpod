# InkScript 言語仕様・実装計画

## 1. 文書の位置付け

この文書は、廃止予定の `.inkbatch` を置き換える UTF-8 テキスト形式
`.inkscript` の言語仕様と実装計画を定める。InkScript は、既存 Batch の
`Input -> Operations -> Output` を包含し、`.inkpod` の journal に保存される
journal-replayable な canonical procedure と等価な文書変更を、別文書へ安全に
再束縛して実行できることを目的とする。

機能要件の最上位の正本は引き続き `SPEC.md` とする。最初のマイルストーン M00 で、
`SPEC.md` に `SCRIPT-*` 要件、この文書への規範参照、`.inkbatch` cutover 前後の状態を
追加する。M00 が完了するまでは production parser、executor、ABI、UI の実装へ進んでは
ならない。この文書と `SPEC.md` が競合する場合は、ユーザーの最新指示、`AGENTS.md`、
`SPEC.md`、この文書の順で解決する。

machine-readable schemaは段階を分ける。M00でregistry schema v1と
`schemas/inkscript/language-v1.json`を規範化し、M07で承認されたexact-current
`schemas/inkscript/registry-schema-v2.json`がcatalog-owned typeを追加してv1を置き換えた。
旧registry schemaは受理しない。language v1は引き続き
registry自体のclosed JSON形式と、command非依存の全language-core type、section、selector、assert、assetの
exact field、型、default、上限を固定する。command entryはowner manifestに従ってM07～M22で
`schemas/inkscript/catalog-v1.draft.json`へ追加する。このdraftはproduct file、clipboard、公開Rust API、
ABI、UIから参照せず、M23で全単射と実装を検証した後に初めて`catalog-v1.json`として批准する。
`docs/inkscript-command-reference.md`はM23以後にlanguage/catalog registryから生成する派生物であり、
手編集しない。

本文中の「必須」「禁止」「拒否」は規範要件である。「推奨」は、同等の安全性、
決定性、保守性を示せる場合に限って置換できる設計判断である。

初期値は次のとおりとする。

| 項目                                |                                       初期値 |
| ----------------------------------- | -------------------------------------------: |
| InkScript file format version       |                                            1 |
| InkScript procedure catalog version | 1（M23で批准予定。批准前はproduction非公開） |
| required replay epoch               |                                           23 |
| native output                       |                      exact-current `.inkpod` |

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
catalog v1 draftはM23までversioned contractではなく、owner milestone内で変更できるがproductから受理
してはならない。M23で批准した後は意味を変更せず、新entryやsignature変更ではcatalog versionを更新し、
旧version拒否test、example、registry、生成referenceを同時更新する。

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
- version 1では`include`、module、別script importを提供しない。完全file一件だけでprogram構造を
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
    -> exact-current .inkpod encode
    -> input ごとの atomic install
```

InkScript は新しい画像処理 engine を持たない。各 `invoke` は既存の typed request /
`CanonicalInvocation` へ正規化され、live commit、Undo/Redo、native replay と同じ primitive
executor を使用する。C++ は parser、selector、画像処理、journal replay を実装せず、
file dialog、path authority、clipboard、UI、thread/job 接続だけを担当する。

InkScript には二つの層がある。

1. **orchestration 層**: inputs、parameters、bindings、output、execution policy
2. **document program 層**: ordered `assert` / `step` と typed canonical invocation

既存 Batch operation は特別な第二 executor として残さず、document program の構文と
command catalog を利用する UI preset に縮退させる。

sourceは編集用のlossless CSTと実行用のsemantic ASTを分離する。外部pathを読む前までの
static compile、authority取得後の`PlanTask`、確認後の`RunTask`は別lifecycleとする。
version 1のitem実行とinstallはimmutable preview順の逐次実行に固定し、Core single-writer、
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
inkscript 1;
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
開始しない。block comment と nested comment は version 1 では提供しない。
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

version 1の出力規則は次のとおりとする。

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

inputs           = "inputs", "{", input_decl*, "}" ;
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
enum_literal     = identifier ;
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

version 1にuser-defined type、alias、generic function、implicit castはない。`type_ref`の
identifierは合成済み`SchemaView`に登録されたbuiltinまたはnamed closed typeへexact-current catalogで
解決する。`language-v1.json`はcommand非依存のbuiltin、stable ID reference、共有enum/record、asset
reference、selector referenceだけを所有する。commandの引数/result専用enum、record、constructorは
そのcatalog entryが所有し、language registryへ逆流させない。

integer tokenは符号付きの数学的整数、decimal tokenは正確な10進有理数として読み、expected
typeへ範囲検査付きで変換する。expected typeなしに整数型を推測せず、decimalを固定幅値へ変換
するのはregistryが指定するcanonical conversionだけとする。enumはexpected closed enumのmemberと
完全一致しなければならない。constructor nameはregistry内でglobalに一意とし、argument count、
argument type、result typeをexactに検査する。合成後のtype名とconstructor名は全entryを通じてglobalに
一意でなければならず、各draft entry追加時とM23 freeze時に重複を拒否する。曖昧なoverload解決を許可しない。

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
    procedure_catalog = 1;
    replay_epoch = 23;
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
    file "cells/A001.inkpod";
    folder "cells/cut-02" {
        cells = range(10, 40);
        recursive = false;
    };
    current_sequence {
        cells = all;
    };
}
```

input kind は次の閉じた集合とする。

- `file`: 一つの native Cell `.inkpod`
- `folder`: 一つの folder 直下にある対応 native Cell file
- `current_document`: command 発行時に固定した一つの document session
- `current_sequence`: command 発行時に固定した sequence/Cut membership

展開itemのorder/name keyは次のとおりとする。`validated filename`は妥当なUnicode scalar列で、native
`.inkpod` extensionを持つ一path componentである。`source_stem`はその最終extensionを除いた非空文字列とし、
output path componentとしての妥当性も検査する。

| item origin                    | `display_label`           | `path_order_key`                       | `source_stem`            |
| ------------------------------ | ------------------------- | -------------------------------------- | ------------------------ |
| `file` / `folder`              | authorized final filename | `ValidatedPathIdentity`のcanonical key | validated filenameのstem |
| `current_sequence` member      | validated member filename | file-backed memberのcanonical key      | member filenameのstem    |
| file-backed `current_document` | backing filename          | backing fileのcanonical key            | backing filenameのstem   |
| pathless `current_document`    | `current-cell.inkpod`     | empty bytes                            | なし                     |

canonical path keyはOS adapterが返すlossless UTF-8、`/`区切り、absolute/root-tag付きの比較専用値である。
Unicode normalization、locale、display用短縮、sourceに書かれた未解決pathを使わない。lossless UTF-8 keyを
作れないfilesystem entryはplan errorとする。pathless current documentでは`duplicate`の空basenameと
`cell_folder = true`をplan errorにする。`new_save`の空basenameは7.9の`cell_<number>.inkpod`を使用できる。

`cells` は `all` または inclusive display-number range `range(first, last)` とする。
display number 0、逆 range、重複 input、同一 file の path alias、非 Cell native file を
拒否する。folder は version 1 では再帰しない。既存Batch互換性のため、全input declarationを
展開してから、全itemをdisplay labelのglobal natural orderで並べる。input declaration順やOS列挙順を
最終順序に使用しない。

natural comparatorはUTF-8 byte列を左から比較する。両側がASCII digitなら連続runを取り、先頭zeroを
除いた桁数、numeric digit bytes、元run長の順に比較する。その他のbyteはASCIIだけlowercase化して
比較する。ここまで同値なら元UTF-8 label bytes、`path_order_key` bytes、document UUID bytesの
順でtie-breakする。Unicode normalization、locale、OS case-foldを使わない。この順序をoutput numbering、
`failure = stop`、report、`wait_ms`の唯一のitem ordinalとする。

relative path は保存済み `.inkscript` の親 directory を基準にする。unsaved source で relative
path を使用する場合、frontend が明示的な base directory を取得するまで実行できない。
`~`、environment variable、implicit current directory、wildcard expansion は使用しない。
absolute path は許可するが、frontend の path authority と preview の対象になる。

command発行時の`CommandContext`はsession/sequence IDとgeneration、document UUID/revision/state digest、
editor revision、ordered membershipを固定する。PlanTaskはCore engine threadでそれらを照合し、一致した
open memberからimmutable `CoreSessionSnapshot`を取得する。snapshotはGenesis、asset store、append-only
journal/branch graph、current StateId/history cursor、全persistent-ID namespaceのnext ID/high-watermark、
document/editor state、両savepointを含む。不一致は`stale_input`で
あり、現在activeな別文書へ再解決しない。dirtyまたはpathlessでもbacking fileへ読み替えず、snapshot取得後の
live編集はplan結果へ影響しない。open `DocumentSession`が所有するmemberは必ずsnapshotを使い、それ以外の
closed memberだけがauthorized file fingerprintを使う。この原子的な照合・capture時点を`plan snapshot time`
と呼び、command issue timeと混同しない。live sessionのpath authorityはsnapshotへ移管しない。

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
既存Batchの「実行ごとに設定」は、operationまたは`editor_group`の編集可能fieldをtyped parameter群へ
loweringするUI convenienceとし、stepに別の実行flagを持たせない。

### 7.5 `bindings` と selector

```inkscript
bindings {
    let paint = select plane {
        layer_kind = binary_coloring;
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

version 1 の selector entity は次の閉じた集合とする。次表は概要であり、exact field、型、
required/default、owner relation、initial-order規則はschema registryと生成referenceを規範とする。

| entity                                 | 主な filter                                                                   |
| -------------------------------------- | ----------------------------------------------------------------------------- |
| `layer`                                | kind、name、initial document order、persistent ID                             |
| `plane`                                | owning layer binding/filter、kind、format、name、initial order、persistent ID |
| `guide`                                | axis、position、persistent ID                                                 |
| `vector_path` / `vector_fill`          | owning plane、persistent ID                                                   |
| `annotation`                           | owning layer、kind、persistent ID                                             |
| `shooting_frame`                       | owning layer、persistent ID                                                   |
| `vanishing_point`                      | owning layer、persistent ID                                                   |
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
            plane = $paint;
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

一つのlegacy Batch operationが複数canonical invocationを実行する場合、operationを一stepへ
押し込めない。Continuous Fillはseedをsource順に一seed一stepへ展開し、enabled seedのstepだけが既存fill
primitiveを一回呼ぶ。disabled seedは`enabled = false`のstepとして保持でき、実行もCommit生成もしない。
enabled seedがN件ならoperation全体で0..N Commitを生成する。UI上の一operationというまとまりは、連続するstepが共有する非意味的な
`editor_group`で保持する。lowering/lifting契約は
`BatchOperation <-> grouped Vec<ScriptStep> <-> Vec<CanonicalInvocation>`である。逆変換は同一groupの
全stepが同じtarget/configを持つseed-fill等、registryがlossless projectionを定義した場合だけ許可する。
その他のadvanced scriptをlegacy Batch operationへ丸めてはならない。

新規 stable object を作成する command は typed result を返せる。

```inkscript
step "Create paint layer" as created_paint {
    enabled = true;
    invoke create_layer {
        kind = binary_coloring;
        name = "Paint";
    };
}

step "Rename created layer" {
    enabled = true;
    invoke set_layer_properties {
        layer = $created_paint.layer;
        visible = true;
        editable = true;
        opacity_milli = 1000;
        name = "Final Paint";
    };
}
```

result fieldと型はschema registryが定義し、scalar、list、roleを失わない。例えば
`$created_paths.paths[0]`のようなconstant indexを許可する。IDはcommit成功時だけ消費する。
disabled producerへの参照はcompile error、`skip_dependents`によるskipped producerへの参照はconsumerも
skip、`only_on_change` resultをno-op/failure後に参照した場合だけ、そのinput itemを
`missing_result`として失敗させる。

`assert` は mutation と Commit を生成しない。version 1 は次を持つ。

- `assert document`: UUID、state digest、ID allocation digest、寸法、DPI、色空間等の既知field
- `assert object`: binding reference と既知 property
- `assert selection`: empty/nonempty と half-open bounds

assert failure はその入力 item を変更せず失敗させる。汎用 boolean expression、条件分岐、
loop は version 1 では提供しない。入力一件ごとの反復だけが暗黙の bounded loop である。

`id_allocation_digest`は全persistent-ID namespaceをregistryのnamespace tag順に並べ、各
`(namespace_tag, next_nonzero_id)`をdomain-separated BLAKE3へ入れた値とする。削除済みIDを含む
high-watermarkを表し、document state digestで代用しない。exact-source fragmentはprogram先頭の
`assert document`でsource UUID、base state digest、ID allocation digestを必須検査する。

version 1のdomain contextはASCII `inkpod.inkscript.id-allocation-digest.v1`とし、BLAKE3 derive-key modeを
使う。hash inputは`namespace_count: u32_le`に続き、registry順で
`tag_length: u16_le`、ASCII tag bytes、`next_nonzero_id: u64_le`を連結する。tagは重複不可で、count/length
overflow、zero/overflow済みnext IDをdigest計算前に拒否する。

### 7.7 InkScript schema registry

M00でregistry schema v1と`language-v1.json`を確定し、command以外の言語coreを固定した。M07で
明示承認されたexact-current `registry-schema-v2.json`は、language v1を変更せずcatalog-ownedな
closed enum／record／constructorを追加し、旧registry schemaを置き換える。
全journal-replayable `PrimitiveId`はowner manifestでM07～M22のちょうど一つへ割り当てる。各ownerは
実装するfamilyのexact command entryと、そのentry専用type/constructorを`catalog-v1.draft.json`へ追加する。
合成`SchemaView`はlanguage-core定義と全catalog entry定義を結合し、type/constructor名の重複を拒否する。
draftはM23まで内部testだけに
使用し、file、clipboard、public Rust re-export、FFI、Windows product commandから受理しない。M23で
全entry、実装、equivalence testの全単射を検証してから`catalog-v1.json`へfreezeし、catalog v1を
production contractとして初めて公開する。

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
返さなければならない。JSON nodeのexact field、tag、path表現、depth/node上限はM00のmeta-schemaを正本とする。

portabilityはcommand名だけの固定値ではない。純粋な
`evaluate_portability(typed_arguments, input_roles, asset_roles)`が次のclassとrequired preconditionを
返す。source authorはclassを指定または緩和できない。

- `portable`: source固有ID/state authorityを必要としない
- `requires_binding`: semantic selectorまたは先行resultへの明示binding後に実行できる
- `strict_source_only`: exact source UUID、base state digest、ID allocation digest等が必須で、v1では
  別文書へrebindできない

classの強さは`portable < requires_binding < strict_source_only`とし、step、group、fragmentは全dependencyの
最大値を取る。fragment内で閉じた先行resultや取り込み済みcontent-addressed assetはclassを強めない。
runtime active object、外部path、clock等への暗黙依存はclassでは正当化できずcatalog errorとする。

Rust variant名、`Debug` output、frontend command ID、localized labelから実行名やfieldを生成しない。
未知command、assert、selector、constructor、enum、fieldは推測せず拒否する。各`PrimitiveId`とprimitive
schemaの組はちょうど一つのentryに対応し、一つのscript commandを複数primitiveの暗黙transactionへ
loweringしない。M23以後はregistry、生成reference、Rust declaration、primitive catalogのdriftをCI failure
とする。M23以前もlanguage/meta-schema、owner manifest、draft entry、実装済みadapter間の局所driftを拒否する。

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
version 1 の canonical `asset` と混同しない。Coreへ渡る procedure は外部 pathを保持しない。

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

`output`はpolicyごとのclosed variantである。`format = inkpod`だけを許可し、一般画像形式を
version 1のBatch outputへ追加しない。

| field          | `duplicate` / `new_save`            | `explicit_overwrite` |
| -------------- | ----------------------------------- | -------------------- |
| `policy`       | 必須                                | 必須                 |
| `format`       | 必須、`inkpod`                      | 必須、`inkpod`       |
| `folder`       | 必須。空stringを許可                | 指定禁止             |
| `cell_folder`  | 必須bool                            | 指定禁止             |
| `basename`     | 必須。空stringを許可                | 指定禁止             |
| `start_number` | 必須`u32`                           | 指定禁止             |
| `direction`    | 必須、`ascending`または`descending` | 指定禁止             |

既存Batch互換のdestination derivationは次のとおりとする。

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
差はdestination namingだけである。新document identityは別の明示fork/new-Genesis仕様を必要とする。

`explicit_overwrite`はopen `DocumentSession`が所有していないfile-backed input自身だけをdestinationに
できる。open sessionのbacking path、current document/sequenceのopen member、別input pathを拒否する。
source/destination identityとopen-session registry generationをplan時とinstall直前に再検査する。
overwriteにはsource上のpolicyだけでなく、planへ一回限りで束縛したpreview/confirmation tokenが必須である。

file identityだけの再検査では、同一identityのまま行われる外部writeを防げない。`explicit_overwrite`は、
planned fingerprintに対するno-lost-update guardをOS adapterが提供できるfilesystemだけで許可する。guardは
最終fingerprint検証からatomic replaceのlinearizationまで、同一identityへのcontent変更、truncate、rename、
delete、置換を排他するか確実に検出しなければならない。RunTaskはguard取得後にvolume/file identity、length、
content digest、native document UUID、利用可能なchange tokenを再検証し、planned値と違えば`stale_input`として
installしない。単なるcheck-then-replaceしか提供できないfilesystemではPlanTaskが
`unsupported_atomic_overwrite`として拒否する。

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
native document UUIDをfingerprintとして固定する。RunTaskはread直前とread後にidentity、length、digestを
再検査し、不一致を`stale_input`としてmutation前に拒否する。静かに最新fileへ読み替えない。

path authorityはfrontendが発行するruntime-only opaque tokenで、source/fragmentへserializeしない。
tokenはauthorized rootまたはexact object、`read | enumerate | create | replace` capability、generation、
`static_compile_digest`、`path_intent_digest`、intent IDを持つ。PlanTaskは検証済みauthorityのIDと
generationを含めてplan digestを生成し、confirmation tokenだけがplan digestへ束縛される。OS adapterは
authority後にhandle-basedでfinal targetを解決し、symlink/reparse
targetがauthorized root内であることを検査する。alias判定はfile identityを優先する。`..`、implicit
cwd、`~`、environment/wildcard/shell expansion、network URL、UNC pathはversion 1で拒否する。Rust Coreは
opaque OS tokenを解釈せず、adapterが検証したbounded path/identity DTOだけを受け取る。

temporary fileもoutputの`create` authority外へ書いてはならない。RunTaskは作成直前にcancel、authority
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

1. current_document/current_sequenceは固定snapshot、file/folderはfingerprint再検査済みのauthorized
   sourceからcurrent native readerで完全にopenする。
2. initial stateでbindingsを解決する。
3. assertとenabled stepを順番に実行する。
4. 各stepを既存canonical executorへ渡す。
5. 完了後のCoreをcurrent `.inkpod` として完全encodeする。
6. cancellation、authority、confirmation tokenを再検査し、検証済みdestination parent handle配下の
   同一volume exclusive temporary fileをwrite/flush/closeする。
7. overwriteではno-lost-update guard下の完全なsource fingerprint、全policyではdestination identity、
   open-session registry、authority、confirmation tokenを再検査してatomic installする。

`CoreSessionSnapshot`は単なる`CellDocument` cloneから新Genesisを作らず、native open/cache-free replayと
同じvalidation経路でstaged Coreへ復元する。既存journal/history/allocatorへscript Commitをappendし、
UUID、Genesis、既存branch、savepointを保持する。出力だけが最終stateのprospective savepointを記録する。

任意の段階のinvalid、failure、cancel、stale、overflow、allocation failureで、その入力の
working Coreとexact temporary fileだけを破棄する。入力file、別item、live current document、
savepointを部分変更しない。`failure` policyは次itemを開始するかだけを決める。

atomicityはdestination file内容についてitem単位であり、job全体や7.9で明記した空directory副作用は
対象外である。atomic create/replace成功をitemのlinearization pointとし、その後に観測したcancelで当該itemを
`cancelled`へ戻さず`installed`と報告し、次item以降だけを`not_started`にする。後続itemのfailure/cancelで
install済みの先行itemをrollbackしない。reportはpreview ordinalごとに
`installed | failed | cancelled | not_started`を必ず一件持つ。
`failure = stop`は失敗itemの後を`not_started`にする。linearization前にcancelを観測したactive itemは
`cancelled`としてinstallせず、残りを`not_started`にする。linearization後は前段の規則に従う。
output `.inkpod`には最終stateを指すprospective document/editor savepointを書くが、source/live sessionの
savepoint、dirty、path authorityを進めない。

dry-runも同じparser、binder、asset ingestion、canonical executorをstaged Coreで実行するが、
encode用temporary fileを作らず、outputをinstallしない。単純なsyntax checkをdry-runと呼ばない。

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
version 1のitem execution、encode、installはimmutable preview順の逐次実行に固定する。item並列化、
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
inkscript_fragment 1;

requires {
    procedure_catalog = 1;
    replay_epoch = 23;
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

Windows clipboardはregistered format `Inkpod.InkScript.v1`へBOMなしUTF-8 byte列とbyte lengthを置き、
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

version 1 は少なくとも次を上限とし、検査付き加算でtotalを計算する。既存Core側のより小さい
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
| fingerprint/readするnative input bytes合計                      |                    64 GiB、またはapplication設定の小さい方 |
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
RunTaskのnative read bytes、timer continuationの残wait budgetも同じcounterへchecked加算する。上限到達後の
parser recovery、Base64 scan、diagnostic生成も残りbudget内に制限する。

scriptは権限境界を拡張しない。path intentをauthority前に表示し、absolute input、script directory外の
asset、output root、replaceを別capabilityとして確認する。network URL、UNC、暗黙download、shell
expansionを行わない。

parser、Base64、descriptor、catalog decoder、selector、fragment dependency collectorは
malformed testとfuzz targetを持つ。allocation failure、cancel、stale input、output collision、
queue saturation、shutdown raceをfault injectionで検証する。

## 13. 完全な例

次はversion 1のsyntaxと、`replace_raster_colors`、`resize_document`の規範的なfieldを示す。
他commandのfieldをこの例から類推して追加してはならず、procedure catalogのexact signatureに従う。

```inkscript
inkscript 1;

requires {
    procedure_catalog = 1;
    replay_epoch = 23;
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
        layer_kind = binary_coloring;
        plane_kind = color;
        cardinality = one;
        missing = error;
    };
}

program {
    step "Replace red" {
        enabled = true;
        invoke replace_raster_colors {
            plane = $paint;
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
            width = 1920;
            height = 1080;
            dpi_x_milli = 144000;
            dpi_y_milli = 144000;
            resample = true;
            anchor = center;
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
  - `inkscript/cst`: Core非依存lossless CSTと局所source edit
  - `inkscript/syntax`: Core非依存semantic syntax AST
  - `inkscript/parser`: recovery付きrecursive-descent parser
  - `inkscript/emit`: typed canonical emitter / fragment writer
  - `inkscript/diagnostic`: stable diagnostic code
- `inkpod-core`
  - `script/catalog`: stable command/assert/selector schema
  - `script/model`: typed semantic program、parameter、binding、result
  - `script/compile`: syntax ASTからstatic model、PathIntentPreview、immutable planへの変換
  - `script/plan`: cancellable input/asset/output planningとconfirmation token
  - `script/bind`: initial document selector解決
  - `script/assets`: bounded asset ingestion
  - `script/execute`: common primitive executorへの接続
  - `script/export`: canonical journalからfragment ASTへの変換
  - `script/report`: preview/dry-run/run report
- `inkpod-ffi`
  - opaque script source/plan/preview/report/export handle
  - source bytes、diagnostics、summary、rowをbatch/span単位でcopyするAPI

`inkpod-format` は `inkpod-core` に依存しない。syntax ASTに `LayerId`、`PixelValue`、
`CanonicalInvocation` を入れず、Core compile境界でtyped valueへ変換する。公開Rust APIは
C ABI recordに依存しない。

### 14.2 Windows

- `ScriptController`は`WorkspaceWindow`のBatch pane単位でsource/plan/task lifecycleを所有する。
- Batch paneはsource ASTのprojectionであり、独自Batch modelを第二の正本にしない。
- file picker、path authority、registered clipboard format、rebind dialogはC++が担当する。
- command発行時のimmutable `CommandContext`へworkspace/session/view/pane/job IDとgenerationを固定し、
  state queryとexecutionで同じtarget解決を使う。
- Core非依存のUTF-8 lex/parseはbounded worker taskで行ってよい。typed compile、snapshot取得、bind、
  execute、export、Core handle registry操作はCore engine threadで行う。
- UI threadはCoreやPresentを同期的に待たず、既存Job Progressへ接続する。
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

`.inkscript` のproduction vertical sliceが既存Batch機能を満たすまでは、`.inkbatch` 実装を
内部shadow比較対象として残してよいが、二つのuser-facing正本を恒久運用しない。

production cutover milestoneでは、Batch pane、open/save filter、resource、clipboardのuser-facing正本を
`.inkscript`へ一本化し、同じ変更で`SPEC.md`、README、file format、architecture、FFI、compatibility、
implementation statusを更新する。`BATCH-*`の過去evidenceと許可された状態値は残し、別欄に
`Superseded by SCRIPT-*`と記録する。`Superseded`を互換状態値として追加しない。同じcutoverで公開
`.inkbatch` open/save/run ABI symbolとfile filterを削除し、C ABI version、header、FFI docs、旧version
拒否testを更新する。旧Batch実装は次milestoneまでtest-private shadow comparatorとしてだけ残せる。

後続の削除milestoneでは、test-privateになった`.inkbatch` parser/writer、BatchGraph persistence、旧runner、
fixtureを削除する。公開symbol/filterの廃止やABI更新をここまで遅延させない。共通algorithmだけをscript
ownerへ移し、production codeからtest-private legacy ownerへの参照を禁止する。
production sourceから旧reader/APIを除去しても、compatibilityの履歴、廃止理由、`.inkbatch` extension/magicを
安全にunsupportedとして拒否するnegative testは残す。docs/tests内の文字列を無差別に全消去しない。

`.inkbatch` reader、one-shot importer、自動migration、互換shimは残さない。

既存 `.inkbatch` の移行機能が必要になった場合は、フォーマットフリーズ前の現行versionのみ
方針に対するユーザーの明示的な別指示を必要とする。この文書だけを根拠に追加してはならない。

## 16. マイルストーン運用規則

### 16.1 状態遷移

各headingのmarkerが実装と利用者受入の正本である。

- `[ ]`: 未実装
- `[~]`: scope、完了条件、自動検証まで完了し、利用者確認または明示承認待ち
- `[x]`: 利用者確認済み、または手動確認不要のdocs-only milestoneが完了
- `[!]`: 利用者確認で不具合あり。同じmilestoneの修正が必要

marker列は常に`[x]*`、高々一つの`[~]`または`[!]`、`[ ]*`の順でなければならない。
完了済みmilestoneより前の未完了、複数の確認待ち、未完了より後の完了を検出した場合は作業を止め、
履歴と差分から状態を修復する。`[!]`の修正が自動検証まで通っても原則`[~]`へ戻し、同じsessionで
後続milestoneを開始しない。

各Codexセッションは、`[!]`があれば最初の一件だけを修正する。なければ直前の`[~]`を処理し、
ユーザーが不具合を併記せず汎用promptを再送した場合は確認成功として`[x]`へ更新してから、最初の
`[ ]`一件だけを実装できる。不具合が併記された場合は`[!]`にして次へ進まない。code/buildへ影響する
milestoneは原則`[~]`で終了し、docs-onlyでも仕様批准やbenchmark承認が必要なら`[~]`で止める。
必要な`[~]`が`[x]`になるまでcompatibilityを`Verified`にしない。

後続milestoneの先行実装、ついでのrefactor、placeholder、stub、disabled UIを追加してはならない。

### 16.2 各sessionの必須事項

各マイルストーンでは次を必須とする。

1. `git status`、ユーザー差分、`SPEC.md`、本書、対象code/testを確認する。
2. 公開契約をtestで先に固定する。
3. milestone内のsuccess、no-op、invalid、cancel/stale/overflowの該当ケースを検証する。
4. format、clippy、test、rustdoc、quick benchmarkを変更範囲に応じて実行する。
5. Windows/FFI/UI変更では実presetsによるCMake build、CTest、smokeを実行する。
6. benchmark workload/envelopeを暗黙変更しない。変更が必要ならそのsessionを止めて承認を求める。
7. 現在状態が変わった文書だけを更新する。testなしで`Verified`にしない。
8. 完了条件をすべて満たした場合だけ状態を`[~]`または`[x]`へ変更し、検証結果を追記する。
9. commit、push、PRは依頼されない限り行わない。
10. 一つのmilestoneを完了したら、後続へ進まず最終報告して停止する。
11. 新しいUIがないbackend milestoneでも、既存binary smokeの確認手順と「新規の手動UI項目なし」を
    最終報告へ明記し、利用者が各session後に回帰確認できるようにする。
12. 最終報告とmilestone記録に次のversion impactを必ず記載する。

```text
Version impact:
- InkScript file:
- InkScript procedure catalog:
- replay epoch:
- .inkpod top-level:
- C ABI:
```

serialized grammar/catalog/ABIを変更する場合はexact-current versionを同じ変更で更新し、旧version拒否testを
追加する。canonical replay semanticsを変える必要が生じた場合は作業を止め、replay epochと`.inkpod`
top-level versionを含む影響を示してユーザー判断を求める。M00で批准したlanguage coreの不足、または
M23でfreezeしたcatalogの不足を発見した場合もsilent fixせず、該当version変更または契約修正として
明示判断する。M23以前のprivate command draftへのowner entry追加・修正はcatalog version変更ではないが、
product file/APIからdraftを受理できない状態を維持する。

### 16.3 開始時version registry

M00はrepositoryのexact-current値と次表を照合し、差があればコードを変えず本書を現状へ更新する。

| contract                    | M00開始時の値 |
| --------------------------- | ------------: |
| InkScript file              |             1 |
| InkScript procedure catalog |             1 |
| replay epoch                |            23 |
| `.inkpod` top-level         |            26 |
| C ABI                       |            14 |

### 16.4 Batch parity traceability

M00で次表を`SPEC.md`要件、compatibility evidence、schema owner manifestへ接続し、M29Cをcutover前の
shadow parity gateとする。

| requirement                                              | 主なowner                     | 必須evidence                                                         |
| -------------------------------------------------------- | ----------------------------- | -------------------------------------------------------------------- |
| `BATCH-001` persisted Input→Operations→Output            | M04–M05B、M11、M28A–M29C      | parse/emit/save/reopen、group/order/enable/set管理                   |
| `BATCH-002` legacy operation全種                         | M07–M09                       | legacy fixture→grouped steps→canonical invocation、direct result一致 |
| `BATCH-003` dry-run/progress/cancel/atomic output/report | M11–M12、M26–M29C             | outcome、temp/output、cancel/failure、report parity                  |
| `BATCH-004` pair/seed/ambiguity/per-run config           | M05A–M05B、M08–M09、M28B–M29C | 1:N fill Commit、exact-depth pair、ambiguity、transient parameters   |

M29Cはinput順、output plan、each-run解決、canonical procedure列、state/composite digest、history、
Undo/Redo、next ID、report、semantic work counter、save/reopen、cache-free replay、failure atomicityを比較する。
旧`.inkbatch`をimportするproduction機能は作らず、旧BatchGraphはshadow testだけで使用する。

## 17. 実装マイルストーン

### [x] M00 — 仕様批准、schema registry、追跡表

**範囲**

- `SPEC.md`へ`SCRIPT-*`要件と本書への規範参照を追加し、cutoverまで`.inkbatch`が現行で
  `.inkscript`は`In progress`であることを記録する。
- `registry-schema-v1.json`へclosed registry JSON AST、formula/evaluator、resource boundを、
  `language-v1.json`へcommand非依存の全language-core type、section、selector、assert、asset schemaを登録する。
- `catalog-v1.draft.json`をprivate draftとして開始する。未実装commandの空entryやstub declarationを作らない。
- 全`PrimitiveId`を一つのowner milestoneと予定equivalence testへ割り当て、
  `BATCH-* -> SCRIPT-* -> milestone -> test`の追跡表を作る。
- version registry、exact/rebound等価性、Continuous Fillの1:N group、benchmark approval手順を批准する。
- production parser、executor、ABI、UIは実装しない。

**完了条件**

- 未定義nonterminal、keyword、escape、language-core type/field、selector、assert、asset schemaがなく、
  command固有type/constructorのcatalog所有規則と全体一意性検査が固定されている。
- current replayable primitiveに未割当、重複割当、Debug由来名がない。
- meta/language schema、private draft、owner manifestのvalidationと、SPEC/追跡表のdrift testが通る。
- catalog draftと未存在のInkScript Rust command declarationとの全体driftを要求せず、productionからdraftを
  参照できないtestがある。
- 利用者の仕様確認待ちとして`[~]`で停止する。

M00のregistry schema v1はM07の明示承認によりexact-current v2へ置換された。M00が批准したlanguage v1、
formula/evaluator semantics、resource boundは変更していない。

**自動検証結果（2026-08-15）**

- closed meta-schema、command非依存language v1、空のprivate catalog draft、84 replayable primitiveの
  owner manifest、Batch parity追跡表を追加した。session-onlyの`LightTableSwapWithActive`だけを明示除外した。
- JSON Schema validationでlanguage／draft／ownerの3 registryをmeta-schemaへ照合し、test-only
  `inkscript_registry` 6件でJSONのduplicate/malformed/overflow拒否、schema/reference閉性、owner全単射、
  version／SPEC／compatibility drift、production非到達性を検証した。
- `cargo fmt --check`、全target／feature Clippy、workspace 467 tests、strict rustdoc、既存approved quick
  benchmarkが成功した。benchmark workload、harness、semantic counter、envelopeは変更していない。
- production parser、executor、Rust public API、C ABI、Windows UIは追加していない。次sessionの不具合報告を
  伴わない汎用promptによりlanguage coreとownership splitが批准され、`[x]`へ移行した。

```text
Version impact:
- InkScript file: 1（初期language contractを登録、bumpなし、批准済み）
- InkScript procedure catalog: 1（private draft、entry 0、bumpなし）
- replay epoch: 23（変更なし）
- .inkpod top-level: 26（変更なし）
- C ABI: 14（変更なし）
```

### [x] M01 — UTF-8 source、lexer、line map、diagnostic

**範囲**

- `inkpod-format`へCore非依存のsource/lexer moduleを追加する。
- BOM、UTF-8、LF/CRLF、comment、keyword、compound literal、escape、maximal munchをbounded tokenizeする。
- UTF-8 byte span、Unicode scalar line/column、stable diagnostic、token/resource limitを実装する。

**完了条件**

- valid/invalid/BOM/CRLF/NUL/escape/token overflow/truncationのpublic API testがある。
- malformed/property/fuzz入口があり、巨大tokenとdiagnostic recoveryをboundedに拒否する。
- parser、Core、FFI、Windows、`.inkbatch`の挙動を変更しない。

**自動検証結果（2026-08-15）**

- `inkpod-format`にcaller byte列を所有copyするimmutable source、BOMをdisplay columnへ数えない
  CRLF-aware line map、UTF-8 byte／1-based Unicode scalar range、21個のstable `INKS-LEX-*` diagnostic、
  exact-current以下へだけ縮小できるresource limits、trivia保持型のbounded maximal-munch lexerを追加した。
- public API test 12件でvalid/no-op、全予約keyword、BOM／CRLF、comment、compound UUID／digest／Base64、
  escape、NUL、invalid UTF-8、standalone CR、leading zero、source／identifier／numeric／string／asset／token／
  diagnostic overflow、recovery、truncation、所有copy、`Send + Sync`を検証した。
- deterministic truncation/malformed corpusを追加し、`inkscript_lexer_v1` libFuzzer targetを宣言して実compileした。
  coverage-guided fuzz実行は行っていない。
- `cargo fmt --check`、workspace全target／feature Clippy、workspace 479 tests（ignored 0）、workspace strict
  rustdoc、既存approved quick benchmarkが成功した。benchmark workload、harness、semantic counter、envelopeは
  変更せず、規範checksumとcounterを維持した。
- parser、CST、semantic AST、canonical emitter、Core、FFI、Windows、`.inkbatch` routeは変更していない。
  Windows configure/build/CTest/smokeは変更範囲外のため再実行せず、既存binaryの回帰確認待ちとして停止する。

```text
Version impact:
- InkScript file: 1（批准済みv1 lexical contractの初回実装、grammar変更なし、bumpなし）
- InkScript procedure catalog: 1（private draft、entry 0、変更なし）
- replay epoch: 23（canonical replay semantics変更なし）
- .inkpod top-level: 26（変更なし）
- C ABI: 14（変更なし）
```

### [x] M02 — lossless CSTとbounded parser

**範囲**

- 完全file/fragment、全section、record/list/value、assert、step、`editor_group`をlossless CSTへparseする。
- trivia、token spelling、error node、source byte rangeを保持し、recovery、duplicate、nesting/node上限を実装する。
- byte-perfect lossless writerを実装する。利用者向け局所source edit APIは実consumerと同じM28Aへ残す。

**完了条件**

- 未編集CSTの`parse -> lossless write`がBOM、CRLF、comment、escape spellingを含めbyte-for-byte一致する。
- invalid sourceからsemantic ASTや実行handleを公開しない。
- duplicate section/member/field、missing member、fragment必須sectionのnegative testがある。

**自動検証結果（2026-08-15）**

- `inkpod-format`へsource借用型の公開lossless CSTとbounded parserを追加し、完全file／fragment、全section、
  input／parameter／binding／assert／step／asset declaration、record／list／reference／constructor／typeをparseする。
  CSTは全token、trivia、元spelling、byte span、error nodeを保持し、未編集writerは元byte列を直接再出力する。
- exact-current versionだけを受理し、file／fragment必須section、fragment禁止section、duplicate section／field／step member、
  missing step member、reserved identifier、空／非連続`editor_group`をstable diagnosticで拒否する。
- CST node、section、nesting、container、aggregate list、reference segment、input／parameter／binding／program statement、
  diagnostic上限をcaller-lowered envelopeで検査し、上限到達時はtruncateせずterminal errorにする。
- public API test 13件でvalid／invalid、BOM／CRLF／comment／escape spellingのbyte-perfect round-trip、全value形、
  exact-current version拒否、recovery、error node、duplicate／missing、全parser resource上限、deterministic truncationを検証した。
  `inkscript_parser_v1` fuzz targetを追加してstandalone manifestの`cargo check`を通した。coverage-guided fuzz実行は行っていない。
- `cargo fmt --check`、workspace全target／feature Clippy、workspace 492 tests（ignored 0）、workspace strict rustdoc、
  既存approved quick benchmark、Windows x64 Debug configure／build／ABI smokeが成功した。benchmark workload、harness、
  semantic counter、envelopeは変更せず、規範checksumとcounterを維持した。
- semantic AST、canonical emitter、Core、C ABI、Windows UI、`.inkbatch` production routeは変更していない。
  通常／日本語UI smokeは変更範囲外として再実行せず、既存binaryの利用者回帰確認待ちで停止する。

```text
Version impact:
- InkScript file: 1（M00批准済みgrammarの初回parser実装、serialized grammar変更なし、bumpなし）
- InkScript procedure catalog: 1（private draft、entry 0、変更なし）
- replay epoch: 23（canonical replay semantics変更なし）
- .inkpod top-level: 26（変更なし）
- C ABI: 14（変更なし）
```

### [x] M03 — semantic ASTとcanonical emitter

**範囲**

- CSTからCore非依存semantic ASTへ変換する。
- 4.4のcanonical emitter、file/fragment writer、deterministic generated name基盤を、schema必須の
  `SchemaView` APIとして実装する。
- schema field orderとliteral正規形をM00 registryから取得する。
- language registryとbounded test command schemaを使う。未登録invokeのcanonical emitは
  `unknown_command_schema`として拒否し、source順や辞書順へfallbackしない。

**完了条件**

- `parse -> semantic AST -> canonical emit -> parse`でsemantic ASTが一致する。
- canonical golden bytesがOS、locale、hash iteration順に依存しない。
- commentを保持する通常保存と、commentを含めない明示canonical emitを混同しないtestがある。

**自動検証結果（2026-08-15）**

- validなlossless CSTだけをCore非依存semantic ASTへ変換し、sectionをschema順、recordをfield-order非依存、
  declarationを意味上のsource順で保持する公開APIを`inkpod-format`へ追加した。invalid/recovery CSTはASTを返さない。
- `language-v1.json`のclosed record／selector／assert field、型、required/default、`canonical_order`をbuild時に
  一方向生成し、bounded private record/command schemaと合成する`SchemaView`を追加した。private catalog draftは
  読み込まず、未知commandは`unknown_command_schema`、未知field／record、欠落required field、重複名／order／typeを拒否する。
- BOMなしUTF-8、LF、末尾改行一つ、section/schema field順、default/`none`省略、数値、string、UUID、digest、
  Base64、constructor、reference、list／recordを4.4の正規形で出力するfile/fragment canonical emitterを追加した。
  lossless writerは元BOM／CRLF／comment／literal spellingを引き続きbyte-perfectに保持し、canonical emitと分離した。
- occurrence順と最小decimal suffixだけに依存するdeterministic generated-name allocatorを追加した。invalid identifierと
  suffix長／counter overflowは名前を予約せず拒否する。
- 公開契約test 6件でfile/fragment round-trip、golden bytes、全literal class、default省略、declaration順、反復決定性、
  invalid CST、schema closure/resource、未知command非fallback、lossless no-op、generated-name collisionを検証した。
- `cargo fmt --check`、workspace全target／feature Clippy、workspace 498 tests（doctest 1、ignored 0）、workspace strict rustdoc、
  M00 architecture/registry gate、既存approved quick benchmarkが成功した。benchmark workload、harness、counter、envelopeは変更せず、
  `canonical_replay=264b98028ac92ac6`、`checkpoint_open=07da1b4e6bc5d289`、`output_color_guard=cfb6b288963c78ba`を維持した。
- Windows x64 Debug configure/build、static CRT、portable ZIP、unsigned MSIX、全36 CTestが成功した。最終増分build後のABI smokeは57.78秒、
  English smokeは174.13秒、Japanese smokeは182.49秒、CTest全体は428.32秒だった。
- typed orchestration、Core compiler/executor、C ABI、Windows UI、`.inkbatch` production routeは変更していない。
  次sessionの不具合報告を伴わない汎用promptにより既存binaryの回帰確認済みとして`[x]`へ移行した。

```text
Version impact:
- InkScript file: 1（M00批准済みsemantic/canonical contractの初回実装、grammar変更なし、bumpなし）
- InkScript procedure catalog: 1（private draft、entry 0、test schemaのみ、変更なし）
- replay epoch: 23（canonical replay semantics変更なし）
- .inkpod top-level: 26（変更なし）
- C ABI: 14（変更なし）
```

### [x] M04 — typed orchestration envelope

**範囲**

- `requires`、`meta`、`inputs`、`output`、`execution`をtyped format modelへ変換する。
- exact-current file/catalog/replay version、metadata extensions、path intent text、range、closed output
  variant、execution policyを検証する。
- `PathIntentPreview`は作れるが外部pathをopenしない。
- production UIにはまだ公開しない。

**完了条件**

- envelopeのround-trip、noncurrent version、unknown field、invalid range/policy/bounds、output variantの
  forbidden field testがある。
- `.inkpod` versionをscript sourceへ固定しない契約がtest/docで一致する。
- 既存`.inkbatch` binaryとUI smokeに回帰がない。

**自動検証結果（2026-08-15）**

- `inkpod-format`へcomplete file専用のimmutable `InkScriptOrchestrationEnvelope`を追加し、`requires`、
  非意味的`meta` extension、全input kindとinclusive cell range、closed native output variant、bounded execution
  policyを固定幅のtyped modelへ変換する。fragment、noncurrent catalog/replay、型不一致、numeric overflow、
  invalid/recursive range、metadata key重複、不正output/executionをsource AST無変更で拒否する。
- file/folder inputとduplicate/new-save/explicit-overwriteから、宣言順の`read`／`enumerate`／`create`／`replace`
  path-intent textをowned previewとして返す。変換とpreviewはfilesystemをopen/列挙せず、authority、Core、task、
  outputを作らない。current document/sequenceと`explicit_overwrite`の静的に不可能な組合せも拒否する。
- procedure catalog versionとrequired replay epochの公開値は`language-v1.json`からbuild生成し、別の手書き正本を
  作らない。`.inkpod` top-level versionはsource/modelへ追加せず、output formatはexact-current native
  `inkpod`だけをtypedに受理する。
- 公開契約test 4件でcanonical round-trip、全input/output variant、metadata extension、path intent、pure no-op、
  noncurrent file/catalog/replay、unknown/forbidden field、invalid type/range/policy/bounds、overflow、failure atomicity、
  `Send + Sync`を検証した。Core/FFI/Windows productからtyped envelopeへ到達しないarchitecture gateを追加した。
- `cargo fmt --check`、workspace全target／feature Clippy、workspace 503 tests（doctest 1、ignored 0）、workspace
  strict rustdoc、lexer/parser fuzz target check、既存approved quick benchmarkが成功した。benchmark workload、
  harness、counter、envelopeは変更せず、M03記録の三checksumを維持した。
- Windows x64 Debug configure/build、static CRT、portable ZIP、unsigned MSIXが成功した。sandbox内でGUI起動を
  要する4 CTestだけが失敗したが、通常desktop権限で対象を再実行して全件成功し、全36 testの成功を確認した。
  ABI smokeは57.78秒、English smokeは168.79秒、Japanese smokeは170.27秒だった。
- typed parameter/program/compiler/executor、path解決／authority、C ABI、Windows UI、`.inkbatch` production routeは
  変更していない。新規の手動UI項目はなく、次sessionの不具合報告を伴わない汎用promptにより既存binaryの
  回帰確認済みとして`[x]`へ移行した。

```text
Version impact:
- InkScript file: 1（批准済みorchestration contractの初回typed実装、grammar変更なし、bumpなし）
- InkScript procedure catalog: 1（private draft、entry 0、変更なし）
- replay epoch: 23（canonical replay semantics変更なし）
- .inkpod top-level: 26（sourceへ固定せず、変更なし）
- C ABI: 14（symbol/record変更なし）
```

### [x] M05A — type、namespace、parameter

**範囲**

- registry type、constructor、closed record、unified value namespace、asset namespaceを実装する。
- typed parameter、literal/default、`ask = each_run`のimmutable run values、binding/asset declarationの
  名前解決を実装する。
- duplicate、undefined、forward、shadowing、parameter/binding dependency cycleを検出する。

**完了条件**

- source range付きtype/constructor/closed-record/range/symbol diagnostic testがある。
- each-run値を確定したrun copyがsource/defaultを変更せず、Cancel/invalidでjob modelを作らない。
- value/asset namespaceと可視性が6.5に一致する。

**自動検証結果（2026-08-15）**

- `language-v1.json`からtype kind、closed enum member、constructor signature／argument constraint、closed-record
  field constraint、selector result typeをbuild時に一方向生成し、既存`SchemaView`だけを型の正本として使う
  immutable `InkScriptDeclarationModel`を`inkpod-format`へ追加した。private catalog draftは読み込まない。
- parameterのdeclared type、literal/default、Q16 exact-decimal ties-to-even、closed sum、list／nullable、constructor
  arity／channel range、closed recordを固定幅のowned typed valueへ変換する。value namespaceはparameter、binding、
  step result aliasで共有し、assetは同名を許す別namespaceとして、duplicate、undefined、forward、shadowing、
  binding dependency cycleをsource declaration range付きのstable diagnosticで拒否する。
- selector bindingはinitial declaration orderだけを使ってparameterと先行bindingを解決し、`cardinality = all`だけを
  `list<entity_ref>`相当へ固定する。selectorのdocument解決／assert／step result availabilityは後続ownerへ残した。
- `ask = each_run`は対象parameterごとの`accepted_default`またはoverrideを必須とするimmutable run copyとして実装した。
  Cancel、欠落、重複、unknown／`ask = never`指定、型不一致はcopyを公開せず、source AST、stored default、declaration
  modelを変更しない。job、Core、path authority、outputは作らない。
- 公開契約test 4件でempty no-op、全主要type／constructor／record、value／asset namespace分離、duplicate／undefined／
  forward／cycle、numeric／Q16／constructor overflow、source range、each-run success／Cancel／invalid atomicity、owned
  `Send + Sync`を検証した。Core／FFI／Windows productからdeclaration/run modelへ到達しないarchitecture gateも追加した。
- `cargo fmt --check`、workspace全target／feature Clippy、workspace 507 tests（doctest 1、ignored 0）、workspace strict
  rustdoc、lexer/parser fuzz target check、既存approved quick benchmarkが成功した。benchmark workload、harness、counter、
  envelopeは変更せず、`canonical_replay=264b98028ac92ac6`、`checkpoint_open=07da1b4e6bc5d289`、
  `output_color_guard=cfb6b288963c78ba`を維持した。
- Windows x64 Debug configure／build、static CRT、portable ZIP、unsigned MSIX、最終binaryで全36 CTestが
  410.99秒で成功した。ABI smokeは58.15秒、English smokeは169.19秒、Japanese smokeは169.96秒だった。
- step result typing／dependency closure、selector／assert実行、Core compiler/executor、C ABI、Windows UI、
  `.inkbatch` production routeは変更していない。新規の手動UI項目はなく、次sessionの不具合報告を伴わない
  汎用promptにより既存binaryの回帰確認済みとして`[x]`へ移行した。

```text
Version impact:
- InkScript file: 1（批准済みlanguage v1 type/namespace/parameter contractの初回実装、grammar変更なし）
- InkScript procedure catalog: 1（private draft、entry 0、変更なし）
- replay epoch: 23（canonical replay semantics変更なし）
- .inkpod top-level: 26（schema／state／replay変更なし）
- C ABI: 14（symbol／record変更なし）
```

### [x] M05B — step result、dependency、fragment closure

**範囲**

- M03のbounded test command schema/`SchemaView`を拡張し、step/group、invoke、scalar/list result reference、
  constant index、asset referenceを実装する。
- disabled producer、result availability、single/list mismatch、全reference edgeのdependency graphを実装する。
- fragment closure、範囲外producerのstrict binding化/拒否、deterministic alpha-renamingを実装する。

**完了条件**

- source range付きresult field/index/availability/dependency diagnostic testがある。
- fragment closureが範囲外mutationを暗黙追加せず、parse/emit/name rewriteがdeterministicである。
- parser/compilerがRust `Debug`名や未知commandを暗黙受理しない。

**自動検証結果（2026-08-15）**

- M03のbounded test command `SchemaView`へclosed scalar／ordered-list result field、availability、canonical orderを
  追加し、immutable step result／typed invocation／contiguous editor groupをM05Aの単一declaration modelへ統合した。
  private catalog draftはentry 0のまま読み込まず、fixed tupleとproduct catalog entryは後続ownerへ残した。
- parameter、binding、先行step result、assetの全reference occurrenceを決定的なsemantic traversal順のbounded dependency graphへ
  正規化した。result field、constant list index、scalar/list exact match、disabled／forward producer、self-cycle、
  `always_on_success`／`only_on_change` availabilityをsource declaration range付きstable diagnosticで検証する。
- step range／`editor_group`のfragment closureは必要なparameter／binding／assetと範囲内producerだけを含め、範囲外
  mutationを暗黙追加しない。範囲外stable resultはsource UUID＋nonzero persistent IDのstrict selector bindingへ
  明示変換するか拒否し、同一asset ID＋descriptorをdeduplicateする。destination collisionはvalue／asset namespaceと
  group keyを分離し、最小decimal suffixで宣言と全referenceを一括alpha-renameする。
- 公開契約test 5件でempty no-op、step/group/result所有、全edge kind、field／index／cardinality／availability、unknown
  schema／command、overflow、caller-lowered resource stop、failure atomicity、外部producer拒否／strict binding化、asset
  dedup、canonical emit→parse→type、反復決定性、`Send + Sync`を検証した。Core／FFI／Windows product非到達gateも拡張した。
- `cargo fmt --check`、workspace全target／feature Clippy、workspace 512 tests（doctest 1、ignored 0）、workspace strict
  rustdoc、lexer/parser fuzz target check、既存approved quick benchmarkが成功した。workload、harness、counter、envelopeは
  変更せず、`canonical_replay=264b98028ac92ac6`、`checkpoint_open=07da1b4e6bc5d289`、
  `output_color_guard=cfb6b288963c78ba`を維持した。
- Windows x64 Debug configure／build、static CRT、portable ZIP、unsigned MSIX、最終binaryの全36 CTestが
  419.29秒で成功した。ABI smokeは58.38秒、English smokeは170.71秒、Japanese smokeは177.70秒だった。
- selector／assert実行、catalog interface、Core compiler/executor、authority／plan、C ABI、Windows UI、`.inkbatch`
  production routeは変更していない。新規手動UI項目はなく、次sessionの不具合報告を伴わない汎用promptにより
  既存binaryの回帰確認済みとして`[x]`へ移行した。

```text
Version impact:
- InkScript file: 1（批准済みresult/reference/fragment closure contractの初回typed実装、grammar変更なし）
- InkScript procedure catalog: 1（bounded test schemaのみ、private draft entry 0、変更なし）
- replay epoch: 23（canonical replay semantics変更なし）
- .inkpod top-level: 26（schema／state／replay変更なし）
- C ABI: 14（symbol／record変更なし）
```

### [x] M06 — selector、assert、catalog interface

**範囲**

- M00 registryからselector entity/filter/owner/order/cardinality/missing policyをtyped modelへ変換する。
- document/object/selection assert、strict UUID/ID/state/ID-allocation preconditionを実装する。
- M03の`SchemaView`を拡張し、portability evaluator、work/result/asset/editor metadataを扱うcrate-internal
  `CatalogView` interfaceを作る。
- test catalogだけを使用し、production empty catalogやstub executorを作らない。

**完了条件**

- one/first/all、missing/ambiguous、owner mismatch、strict UUID/state、list resultをcrate-internal integration
  APIで検証する。
- `skip_dependents`が同じdependency graph上でassert/stepへ推移し、静的disabled依存と混同されない。
- query/view/session commandをcatalog interfaceが受理しない。
- M23までpublic re-export、FFI、Windows product routeから到達不能であるvisibility testがある。

**自動検証結果（2026-08-15）**

- M00 language registryからselector entity/filter/owner/order/cardinality/missing policy、assert field/comparison、
  persistent-ID namespace順を生成typed metadataへ変換した。JSON grammarとprivate catalog draftは変更していない。
- crate-internal initial-document snapshot APIで、initial orderに基づく`one`／`first`／`all`、semantic filter、
  owner relation、strict source UUID＋persistent ID、document state／ID-allocation digest、object／selection assertを
  mutationなしで検証する。ID-allocation digestは批准済みBLAKE3 derive-key規則とregistry namespace順を使用する。
- `skip_dependents`をbinding、assert、step、result consumerが共有するdependency graphで推移させ、静的disabled stepは
  `Disabled`のまま区別した。missing、ambiguous、owner mismatch、stale precondition、invalid snapshot、overflow、
  resource bound、atomic failureをcrate-private testで固定した。
- test-only constructorだけを持つcrate-internal `CatalogView`へclosed portability/work formula、result、asset、editor
  metadataを実装した。document mutationだけを受理し、query/view/session command、未知path、除算0、overflow、
  nesting/rule上限を拒否する。production empty catalogとstub executorは作っていない。
- 新規2 catalog test、4 binding/assert test、1 crate-internal integration contractを含むworkspace全519 test、
  `cargo fmt --check`、all-target/all-feature Clippy（warning deny）、strict rustdoc、lexer/parser fuzz target buildが成功した。
  Core/FFI/Windowsと`inkpod-format` public re-exportからprivate preparation/catalog APIへ到達できないgateも成功した。
- 承認済みquick benchmarkはworkload、harness、semantic counter、envelopeを変更せず成功し、
  `canonical_replay=264b98028ac92ac6`、`checkpoint_open=07da1b4e6bc5d289`、
  `output_color_guard=cfb6b288963c78ba`を維持した。
- Windows x64 Debug configure／build、static CRT、portable ZIP、unsigned MSIX、最終binaryの全36 CTestが
  407.53秒で成功した。ABI smokeは58.03秒、English smokeは167.84秒、Japanese smokeは168.35秒だった。
- command entry、Core-backed compiler/executor、authority／plan、C ABI、Windows UI、`.inkscript` product route、
  `.inkbatch` production routeは変更していない。新規手動UI項目はなく、次sessionの不具合報告を伴わない
  汎用promptにより既存binaryの利用者回帰確認済みとして`[x]`へ移行した。

```text
Version impact:
- InkScript file: 1（批准済みselector/assert language v1 contractの初回typed実装、grammar変更なし）
- InkScript procedure catalog: 1（crate-internal test catalogのみ、private draft entry 0、変更なし）
- replay epoch: 23（canonical replay semantics変更なし）
- .inkpod top-level: 26（schema／state／replay変更なし）
- C ABI: 14（symbol／record変更なし）
```

### [x] M07 — legacy simple catalog adapter

**承認gate（2026-08-15、registry schema v2案を承認済み）**

- M07着手時、M00で批准した`registry-schema-v1.json`の`catalog_registry`が`entries`だけを所有し、
  7.7でcatalog ownerへ割り当てたcommand専用`enum`／`record`／`constructor`を表現できないことを確認した。
  `mirror_document`、`rotate_document`、`resize_document`のaxis／direction／anchorをclosed typed argumentとして
  登録できず、`u32`、`bool`、別用途の`guide_axis`で代用するとexact entryとunknown-enum拒否契約を満たさない。
- 承認案はexact-current registry schemaをv2へ上げ、catalog registryへclosedな`enums`／`records`／
  `constructors`を追加すること。`language-v1.json`のlanguage coreと`.inkscript` grammarは変更せず、M23前の
  private draft entry追加としてInkScript file／procedure catalog／replay epoch／`.inkpod`／C ABI versionは
  据え置く。旧registry schemaを暗黙受理する互換routeは作らない。
- M00の`registry-schema-v1.json`を同名でin-place変更する代替案と、typed contractを弱める数値／boolean代用は
  不採用とした。旧schemaの互換reader／shimは作らない。

**範囲**

- owner manifestで割り当てられたvisibility/property、plane conversion、mirror、rotate、resizeのexact entryを
  catalog v1 draftへ追加し、adapterを実装する。
- argument/result/portability/work/editor metadataをtyped invocationと双方向変換する。
- 既存primitive semanticsとM00 language schemaを変更しない。

**完了条件**

- 対象fixtureで`CanonicalInvocation -> ScriptStep -> CanonicalInvocation`とdirect state digestが一致する。
- unknown field/type/enum、format/target mismatchを原子的に拒否する。
- 実行engineとUIはまだ切り替えない。

**自動検証結果（2026-08-15）**

- 承認済みregistry schema v2へcatalog-owned closed enum／record／constructorを追加し、language v1 grammarを
  変更せずlanguage、owner manifest、private catalog draftをexact-current v2へ更新した。旧v1 schema fileは
  廃止し、旧version不在／拒否をregistry testで固定した。
- owner manifestのM07割当どおり、property 2件、plane/layer conversion 2件、mirror／rotate／resize 3件の
  ちょうど7 entryをprivate catalog v1 draftへ追加した。stable name、`PrimitiveId`、schema／semantics revision、
  argument/result、portability、checked work formula、cancellation boundary、editor metadata、equivalence IDの
  全単射とtype closureを検証する。
- Core-private adapterはtyped stepとissue-time stable bindingをcanonical invocationへ双方向変換するだけとし、
  実行は既存の単一canonical executorへ委譲する。第二canonical model／第二executor、private catalog entryの
  public re-export、FFI／Windows／`.inkscript` product routeは追加していない。
- 7 primitiveすべてでdirect実行とtyped step経由のdocument digest、state、revision、history、next ID、savepointを
  比較した。no-op、invalid、stale target、resource bound、unknown field/type/enum、format/target mismatch、
  failure atomicity、`Send + Sync`、product非到達性を固定した。M09以前のためrunner cancellationとnative outputは
  対象外である。
- `cargo fmt --check`、workspace全target／feature Clippy（warning deny）、workspace 527 tests（doctest 1、
  ignored 0）、workspace strict rustdoc、全format fuzz target checkが成功した。
- 承認済みquick benchmarkはworkload、harness、semantic counter、envelopeを変更せず成功し、
  `canonical_replay=264b98028ac92ac6`、`checkpoint_open=07da1b4e6bc5d289`、
  `output_color_guard=cfb6b288963c78ba`を維持した。
- Windows x64 Debug configure／build、static CRT、portable ZIP、unsigned MSIX、最終binaryの全36 CTestが
  407.99秒で成功した。ABI smokeは57.70秒、English smokeは168.09秒、Japanese smokeは168.80秒だった。
- 自動検証完了後、次sessionの不具合報告を伴わない汎用promptにより既存binaryの利用者回帰確認済みとして
  `[x]`へ移行した。

```text
Version impact:
- Registry schema: 2（catalog-owned enum／record／constructor追加、旧v1拒否）
- InkScript file: 1（language v1 grammar／serialized program変更なし）
- InkScript procedure catalog: 1（M23前のprivate draftにowner exact entry 7件、version変更なし）
- replay epoch: 23（canonical replay semantics変更なし）
- .inkpod top-level: 26（schema／state／replay変更なし）
- C ABI: 14（symbol／record変更なし）
```

### [x] M08 — legacy image catalogとgrouped Batch adapter

**範囲**

- owner manifestで割り当てられたcolor replace、Continuous Fill、separation、line width、全filter、
  boundary airbrush、dust removalのexact entryをcatalog v1 draftへ追加し、adapterを実装する。
- exact-depth pair、multi-row seed/pair、typed destination/missing policyを双方向変換する。
- Continuous Fillを一seed一stepへ展開し、同じ`editor_group`からだけlosslessにlegacy operationへ戻す。
- M07 entryを再登録・再実装しない。

**完了条件**

- `BatchOperation -> grouped Vec<ScriptStep> -> Vec<CanonicalInvocation>`と、投影可能な逆方向が一致する。
- enabled/disabled seed/pair、0..N Commit、exact-depth、ambiguity、native separation fixtureが一致する。
- advanced groupをlegacy operationへlossy変換せず、明示的にprojection不能と診断する。

**自動検証結果（2026-08-15）**

- owner manifestがM08へ割り当てた6 primitiveのexact entryをprivate catalog v1 draftへ追加し、draftを
  13/84 entryへ拡張した。line widthは既存`VectorCorrectWidth` canonical invocationへ投影するが、catalog
  entryのownerはM20のままとし、M08で重複登録しない。
- Core-private adapterは全M08 image payloadをclosed typed recordへ双方向変換し、M07 stepを再利用する。
  Continuous Fillは一seed一step、pair/seed enable、exact-depth color、native separation destination、targetの
  missing policyとper-run flagを保持し、同一`editor_group`でないadvanced groupの逆投影を拒否する。
- grouped adapter／image codecの10 testと9 registry contractでsuccess、semantic no-op、invalid、cancel、
  stale、resource overflow、missing、ambiguity、atomicity、ownership/thread、direct canonical routeとのdigest／
  state／revision／history／next-ID／savepoint一致、private product非到達性を検証した。
- Rust workspace 538 test、format、Clippy、strict rustdoc、format fuzz target check、承認済みquick benchmark、
  Windows x64 Debug configure／build／static CRT／MSIX／portable ZIP、および全36 CTestが成功した。
  CTestは428.74秒、ABI smokeは71.62秒、English smokeは171.25秒、Japanese smokeは172.21秒だった。
- M08はproduct UI／ABI／`.inkscript` file routeを追加しない。次sessionの不具合報告を伴わない汎用promptにより、
  既存binaryのBatch回帰とInkScript UI非公開の利用者確認済みとして`[x]`へ移行した。

```text
Version impact:
- Registry schema: 2（M07で批准済みのexact-current schemaを使用、変更なし）
- InkScript file: 1（language v1 grammar／serialized program変更なし）
- InkScript procedure catalog: 1（M23前のprivate draftにowner exact entry 6件、version変更なし）
- replay epoch: 23（canonical replay semantics変更なし）
- .inkpod top-level: 26（schema／state／replay変更なし）
- C ABI: 14（symbol／record変更なし）
```

### [x] M09 — Core compiler、binding、単一入力staged実行

**範囲**

- semantic ASTからstatic typed programをcompileし、parameter freeze、aggregate invocation/work/ID/journal
  budgetを検査する。
- initial document selector binding、`skip_dependents`、assert、result availabilityを実装する。
- M07/M08 stepを既存canonical executorで一つずつstaged Coreへ実行する。
- 単一のin-memory/native inputについてdry-run reportを返す。まだmulti-file installは行わない。

**完了条件**

- success/no-op/invalid/missing/ambiguous/skip/cancel/stale/ID非消費をcrate-internal integration APIから検証する。
- M23完了までcompile/bind/run APIを`inkpod-core`のpublic re-export、FFI、Windows、product commandへ
  公開しない。
- stepごとのCommit/Undo/revision、result ordinal、ID high-watermark、最終state digestがdirect canonical routeと一致する。
- failure時にsource Coreとstaged resultが公開されない。

**自動検証結果（2026-08-15）**

- semantic AST、typed declaration、run parameterを一つのimmutable static programへcompileし、parameter参照を
  recursiveにfreezeした。command ownerのclosed schemaを合成し、checked invocation／work／output ID／asset／
  output-growth budgetと、実行直前のprocedure／state／journal-event／branch／stable-ID high-watermarkを検査する。
- initial document snapshotからlayer／plane selectorをdocument-tree順でbindし、`one`／`first`／`all`、
  `missing = error`／`skip_dependents`、document／object／selection assertを既存M06 evaluatorで解決する。
  result availabilityとoutput ordinalはtyped result contractから決定し、failure時はpartial binding/reportを返さない。
- M07／M08 typed stepをowner adapterで既存`CanonicalInvocation`へlowerし、staged `Core`上の単一canonical executorへ
  一stepずつ委譲する。in-memory captureはissue-time fingerprintを再検査し、native inputはexact-current `.inkpod`
  bytesをstaged Coreへdecodeする。source Core、通常savepoint、path authority、filesystem outputは変更しない。
- end-to-end contractでassert、success、semantic no-op、missing、ambiguity、skip、invalid native bytes、実行前／
  step間Cancel、stale capture、counter overflow、ID非消費、Undo／Redo、native save-image decode、direct canonical routeとの
  per-step Commit／revision／history／journal／next-ID／document/editor savepoint／最終state digest一致を検証した。
- compile／bind／runとreport型はcrate-privateのままで、`inkpod-core` public re-export、C ABI、Windows、product command、
  `.inkscript` file-open routeへ公開していない。private catalog draftは13/84 entryのままである。
- `cargo fmt --check`、workspace全target／feature Clippy（warning deny）、workspace 542 tests（doctest 1、
  ignored 0）、workspace strict rustdoc、全format fuzz target checkが成功した。承認済みquick benchmarkはworkload、
  harness、semantic counter、envelopeを変更せず、全10 checksumと意味counterを維持した。
- Windows x64 Debug configure／build、static CRT、portable ZIP、unsigned MSIX、および最終binaryの全36 CTestが
  407.05秒で成功した。ABI smokeは57.34秒、English smokeは168.45秒、Japanese smokeは168.09秒だった。
- 自動検証完了後はproduct binaryから到達できない内部milestoneとして`[~]`で停止し、次sessionの不具合報告を
  伴わない汎用promptにより既存binaryのBatch回帰とInkScript UI非公開の利用者確認済みとして`[x]`へ移行した。

```text
Version impact:
- Registry schema: 2（exact-current schemaとowner metadataを使用、変更なし）
- InkScript file: 1（language v1 grammar／serialized program変更なし）
- InkScript procedure catalog: 1（private draft 13/84、entry追加なし、version変更なし）
- replay epoch: 23（canonical replay semantics変更なし）
- .inkpod top-level: 26（decode／staged replayのみ、schema／state／replay変更なし）
- C ABI: 14（symbol／record変更なし）
```

### [x] M10 — canonical asset decodeとimmutable freeze

**範囲**

- inline Base64と、callerが検証済みidentity付きで渡すbounded `AuthorizedAssetStream`からraw canonical
  payloadを取り込む。filesystem pathやOS authority tokenをCoreで解釈しない。
- language registryのdescriptor、AssetId、length、duplicate、個別/総量を検証する。
- 全assetをRust-owned immutable storeへfreezeし、asset roleを使うtyped planとportability評価の基盤を作る。
- external pathのopen/identity解決とScriptExecutionPlanへの接続はM11へ残す。

**完了条件**

- inline/authorized streamの同一assetが同じidentityとlogical bytesを返す。
- digest mismatch、truncation、oversize、duplicate descriptor mismatch、cancelでasset/commit/outputを公開しない。
- payload read/copy counterと既存resource envelopeに回帰がなく、production commandのstubを作らない。

**自動検証結果（2026-08-15）**

- `data`／`data_file`のexactly-one制約をapproved language-v1 descriptorからtyped modelへ実装した。inline Base64と、
  opaqueな検証済みidentityを持つbounded `AuthorizedAssetStream`を同じdescriptor検査と既存Core `AssetStore`へ通し、
  path、OS authority token、handleをCoreで解釈または保持しない。
- declared `AssetId`、canonical descriptor、stride／logical length、個別／inline総量／全asset総量、duplicate descriptorを
  read前に検査する。64 KiB以下のchunkでcancelを検査しながら読み、前後identity一致、truncation、余剰byte、reader error、
  不正read countを拒否する。全assetを局所storeへfreezeした後だけimmutable `FrozenScriptAssets`を返すため、失敗時はasset、
  commit、outputを一切公開しない。
- asset symbolはRust-owned immutable recordへaliasし、catalogのasset role／inline-external policyを検査するtyped role planと
  portability summaryを作る。payload read／copy、inline decode、logical byte、declaration／unique assetのexact counterを返す。
  external path open／identity解決と`ScriptExecutionPlan`への接続はM11へ残した。
- Core-private 4 testとformat public-model 1 testでinline／authorized identity・logical-byte一致、empty no-op、invalid descriptor、
  digest mismatch、truncation、余剰byte、oversize、combined limit、duplicate mismatch、cancel、stale identity、read failure、
  malformed count、counter、role policy、Rust ownership／`Send + Sync`、product非到達性を検証した。
- `cargo fmt --check`、workspace全target／feature Clippy（warning deny）、workspace 547 tests（doctest 1、ignored 0）、workspace
  strict rustdoc、全format fuzz target checkが成功した。承認済みquick benchmarkはworkload、harness、semantic counter、
  envelopeを変更せず、全10 checksumと意味counterを維持した。
- Windows x64 Debug configure／build、static CRT、portable ZIP、unsigned MSIX、および最終binaryの全36 CTestが
  405.02秒で成功した。ABI smokeは57.50秒、English smokeは167.21秒、Japanese smokeは167.48秒だった。
- asset ingestion／freeze／role planはcrate-privateのままで、M09 compiler、Core public re-export、C ABI、Windows、product command、
  `.inkscript` file-open routeへ接続していない。前回`[~]`で停止した後、今回promptにより既存Batch回帰と
  InkScript UI非公開に問題がなかったことを利用者確認済みとして`[x]`へ移行した。

```text
Version impact:
- Registry schema: 2（exact-current resource値とdescriptor制約を使用、schema変更なし）
- InkScript file: 1（批准済みasset descriptor／exactly-one／resource semanticsの初回実装、grammar変更なし）
- InkScript procedure catalog: 1（private draft 13/84、asset role entry追加なし、version変更なし）
- replay epoch: 23（canonical replay semantics変更なし、既存AssetId規則を使用）
- .inkpod top-level: 26（schema／state／replay変更なし）
- C ABI: 14（symbol／record変更なし）
```

### [x] M11 — PathIntent、authority DTO、immutable input/output PlanTask

**範囲**

- Static compileからpath intentを作り、authority後のcancellable PlanTaskを実装する。
- file/folder/current document/current sequenceのplan-snapshot/fingerprintを固定し、external asset streamを
  M10 ingestionへ接続する。
- OS非依存の`ValidatedPathIdentity`と`AuthoritySnapshot` DTO、注入可能なtest adapterを使い、Coreがopaque
  token/handleを解釈しないままnatural order、range、dedup、alias結果、output naming/collision、
  open-session overwrite拒否を計画する。Windows handle/reparse adapterはM27Aで接続する。
- static/path-intent digestへ束縛したauthorityからplan digestを作り、scope、input/output identity、authority
  generationへ束縛したconfirmation tokenを作る。このmilestoneではoutputを作成・installしない。

**完了条件**

- dirty/pathless current document、sequence membership変更、file identity replacement、injected alias、open
  session overwrite、asset path change、number overflow/underflowのtestがある。
- matchしないentryを大量に含むfolderでentry/name-byte/work/depth limitを検査し、truncateせずplan全体を
  無変更で拒否するtestがある。
- PlanTaskのcancel/failureがCore、directory、temporary、destinationを変更しない。
- preview順とoutput namingが既存Batch fixtureに一致し、authorityとplan digestに循環依存がない。

**自動検証結果（2026-08-15）**

- static compileはfile／folder input、external asset、output create／replaceのintentへstable IDを付け、
  `static_compile_digest`と独立した`path_intent_digest`を作る。empty non-output path、UNC／URL、home shorthand、
  wildcard、parent traversalをauthority取得前に拒否し、source textをauthorityとして扱わない。
- OS非依存の`ValidatedPathIdentity`、`AuthoritySnapshot`、session／sequence expectation、native fingerprint、
  open-session set snapshot、folder scan DTOと注入可能なadapterを追加した。Coreはpath canonical keyと固定幅の
  volume／object／alias／generationだけを検査し、OS token、handle、reparse固有型を解釈または保持しない。
- cancellable PlanTaskはfile／folder／dirtyまたはpathless current document／current sequenceをimmutable snapshotへ
  固定する。open session所有fileはCore snapshotへ切り替え、closed fileだけをfingerprintとして保持する。
  folderのmatch/nonmatch entry、name bytes、work、depth、native read、expanded item、invocation、wait、logical output＋
  temporaryをchecked集計し、超過時にtruncateせずplan全体を公開しない。
- 全input展開後にapproved natural order、range、document UUID／path alias dedupを適用し、既存Batch互換の
  duplicate／new-save／explicit-overwrite namingをchecked `u32`で導出する。input／asset／script／open session／
  item間collision、open-session overwrite、guard不能overwrite、identity replacementを出力作成前に拒否する。
- M10 external asset streamをauthorityのexact object identityへ束縛してimmutable storeへfreezeし、plan digestは
  static/path digestの後にauthority ID/generation、session/input fingerprint、asset、destinationを取り込む。
  confirmation tokenはplan digestとall/current scopeへ束縛し、一回だけ消費できる。authorityはplan digestを参照しないため
  循環依存しない。
- Core-private 4 contractでsuccess／empty program no-op、invalid path、range、natural order、output naming、dirty/pathless
  snapshot、sequence stale、file replacement、alias duplicate、open-session overwrite、asset stale、output collision、
  number overflow/underflow、entry/name/work/depth resource、cancel、adapter failure、atomic nonpublication、Send owner、
  one-shot/scope confirmation、digest orderingを検証した。filesystem mutation API、temporary、destination writeは存在しない。
- `cargo fmt --check`、workspace全target／feature Clippy（warning deny）、workspace 551 tests（doctest 1、ignored 0）、
  workspace strict rustdocが成功した。承認済みquick benchmarkはworkload、harness、semantic counter、envelopeを変更せず、
  全10 checksumと意味counterを維持した。
- Windows x64 Debug configure／build、static CRT、portable ZIP、unsigned MSIX、および最終binaryの全36 CTestが
  415.54秒で成功した。ABI smokeは57.96秒、English smokeは171.86秒、Japanese smokeは172.41秒だった。
- plan／authority DTO／adapter／confirmationはcrate-privateで、Core public re-export、C ABI、Windows、production Batch、
  `.inkscript` file-open routeへ接続していない。M12のRunTask、native encode、temporary／install、reportは未実装である。
  前回`[~]`で停止した後、今回promptにより既存Batch回帰とInkScript UI非公開に問題がなかったことを
  利用者確認済みとして`[x]`へ移行した。

```text
Version impact:
- Registry schema: 2（approved language-v1 resource／path contractの実装、schema変更なし）
- InkScript file: 1（grammar／serialized field／selector意味変更なし）
- InkScript procedure catalog: 1（private draft 13/84、entry／signature変更なし）
- replay epoch: 23（canonical invocation／state／pixel semantics変更なし）
- .inkpod top-level: 26（encode／install／schema／replay変更なし）
- C ABI: 14（symbol／record変更なし）
```

### [x] M12 — sequential multi-item runnerとnative persistence

**範囲**

- preview順のstaged open/execution、current `.inkpod` encode、same-volume temporary、atomic installを接続する。
- dry-run、progress、cancel、continue/stop、timer continuationによるwait、deterministic reportを実装する。
- install直前のsource/destination/open-session/authority/token再検査と、atomic install後のlinearizationを実装する。

**完了条件**

- per-item atomicity、installed/failed/cancelled/not-started、cancel/encode/save failure、duplicate/overwriteのtestがある。
- fake OS adapterでsame-identity content updateをoverwrite guardが検出するtest、guard不能filesystemが
  `unsupported_atomic_overwrite`になるtest、temp create前のauthority/cancel失効とwriter close後のtemp
  identity swapを拒否するtestがある。
- 当初存在しない共有folderへ複数itemを逐次installし、job-created directory identityを再利用するtestと、
  atomic create/replace直前・直後へbarrierを置いたcancel race testがある。
- dry-runがdirectory/temporary/outputを作らず、先行installed itemを後続failureでrollbackしない。
- outputをsave/reopenし、cache-free replay、Commit列、Undo/Redo、next ID、document/editor savepointが一致する。
- dirty/pathless current documentの`CoreSessionSnapshot`から既存Genesis、journal/branch、history cursor、asset、
  next IDを保持したままscript Commitをappendし、save/reopenできるtestがある。
- source/live document、dirty、savepoint、path authorityをsuccess/failure/cancelで変更しない。

**自動検証結果（2026-08-15）**

- one-shot confirmationを消費してimmutable planのscopeを固定し、preview ordinalごとに一件ずつ進む
  Core-private `ScriptRunTask`を追加した。item completionと`wait_ms` timer要求を別advanceとして返すため、
  Core engine threadをsleepさせず、item execution／encode／install順は常にpreview順になる。
- file inputはread前後の完全fingerprint、raw byte length／BLAKE3、native document UUIDを照合してcurrent v26を
  staged openする。session inputはM11の完全snapshotからCoreを復元し、既存Genesis、asset、journal／branch、
  history cursor、persistent-ID high-watermark、document/editor savepointへ既存canonical executorのCommitをappendする。
- install modeは最終document/editor stateをprospective savepointとしてcurrent `.inkpod`へencodeする。runtime-only
  adapter境界はauthority／open-session generation、source／destination identity、same-volume temporary、write／flush／close後
  identity、atomic create／replace、overwrite no-lost-update guardを再検査し、OS handleやpath tokenをCoreへ入れない。
- atomic installだけをlinearization pointとし、直前Cancelはactive itemを`cancelled`、直後Cancelは当該itemを
  `installed`のまま後続を`not_started`にする。`failure = continue | stop`、installed／failed／cancelled／not-started、
  作成directory、item execution reportをpreview順で決定的に保持し、先行installed itemをrollbackしない。
- dry-runは同じstaged open／binding／canonical executorを通るが、encode、directory、temporary、destinationを作らない。
  output byte上限、adapter save failure、stale authority／session／source／destination、atomic capability消失はitem単位で
  fail closedになり、temporary identity swap時は別objectをcleanupしない。
- Core-private 4 integration contractと1 ownership contractでnatural-order 3-item run、shared missing directory再利用、
  nonblocking wait、continue／stop、encode resource／save failure、atomic install前後Cancel、guard不能overwrite、同一identityの
  content更新、temp swap、authority失効、temp作成前Cancel、dry-run無書込み、`Send` ownershipを検証した。
- native outputをcache-free reopenし、canonical direct routeとstate／Commit／journal／Undo／Redo／next IDを照合した。
  dirty／pathless snapshot fixtureはasset-backed Genesisとinactive branchを持ち、sourceのdirty／path／両savepointを変えず、
  outputだけが最終document/editor savepointを持つことを検証した。
- `cargo fmt --check`、workspace全target／feature Clippy（warning deny）、workspace 556 tests（doctest 1、ignored 0）、
  strict rustdocが成功した。承認済みquick benchmarkはworkload、harness、semantic counter、envelopeを変更せず、
  全10 checksumと意味counterを維持した。
- Windows x64 Debug configure／build、static CRT、portable ZIP、unsigned MSIX、および最終binaryの全36 CTestが
  410.11秒で成功した。ABI smokeは57.27秒、English smokeは168.42秒、Japanese smokeは169.34秒だった。
- RunTask／report／OS adapter契約はcrate-privateで、実Windows authority adapter、Core public re-export、C ABI、UI、
  production Batch、`.inkscript` file-open routeへ接続していない。前回`[~]`で停止した後、今回promptにより既存Batch回帰と
  InkScript UI非公開に問題がなかったことを利用者確認済みとして`[x]`へ移行した。

```text
Version impact:
- Registry schema: 2（approved language-v1 execution／atomic-install contractの実装、schema変更なし）
- InkScript file: 1（grammar／serialized field／selector意味変更なし）
- InkScript procedure catalog: 1（private draft 13/84、entry／signature変更なし）
- replay epoch: 23（既存canonical executor／invocation／state／pixel semantics変更なし）
- .inkpod top-level: 26（exact-current encoder／decoder再利用、schema／replay変更なし）
- C ABI: 14（symbol／record変更なし）
```

### [~] M13 — InkScript performance contract proposal

**範囲**

- quick/full workload、input、seed、work formula、semantic counter、checksum、測定環境、sample数、
  proposed envelopeを文書化する。
- 既存benchmark harnessへの予定差分を提示し、このmilestoneではharness/envelopeを変更しない。

**完了条件**

- workloadがparser/compile/bind/runner/assetの意味counterを測り、private fieldやwall-clockだけに依存しない。
- proposalと全探索測定を記録して`[~]`で停止する。次の汎用promptはproposalの明示承認を兼ねる。

**提案・自動測定結果（2026-08-15、明示承認待ち）**

- [`docs/inkscript-performance-proposal.md`](docs/inkscript-performance-proposal.md)へquick／fullのsource、固定seed、
  step／item／asset入力、catalog work formula、parser／compile／bind／runner／asset counter、FNV-1a checksum、
  timed interval、独立process sample規則、測定環境、提案envelopeを記録した。
- quickは128 step、4 success item、256 KiB inline assetとし、failure／cancel probeを含む6 attempted itemで
  token 7,965、CST node 2,000、dependency edge 128、statement 774、invocation 768、Commit／no-op各384、
  installed／failed／cancelled = 4／1／1、cache-free reopen 4を固定する。checksumは`0f84d2c54cfe1e2c`である。
- fullは1,024 step、8 success item、16 MiB inline assetとし、10 attempted itemでtoken 61,725、CST node 15,440、
  dependency edge 1,024、statement 10,250、invocation 10,240、Commit／no-op各5,120、installed／failed／cancelled =
  8／1／1、cache-free reopen 8を固定する。checksumは`17c636b92b1aebf1`である。
- step 32／64／128／256／512／1,024／2,048、item 1／2／4／8／16、asset 16 KiB／64 KiB／256 KiB／1 MiB／
  4 MiB／16 MiBの全候補軸を一時的なcrate-private Release probeで測定し、選択full複合も測定した。probeは測定後に
  完全削除し、production code、test、Cargo target、既存benchmark harness／checksum／envelopeへ差分を残していない。
- Windows x64 Ryzen 9 9950X3D Releaseで各profileのwarm-up 1 processを破棄後、quick 9 process中央値は
  85,372,200 ns、full 5 process中央値は20,455,099,800 nsだった。全sampleが同じcounter／checksumを維持し、
  既存x64契約と同じ丸めた75–125%幅からquick 64–107 ms、full 15.3–25.6 sを提案する。
- 一時probe除去後の`cargo fmt --check`、workspace全target／feature Clippy（warning deny）、workspace 556 tests
  （doctest 1、ignored 0）、workspace strict rustdocが成功した。既存approved quick benchmarkも10 scenarioの
  checksum／semantic counterをすべて維持し、workload、harness、payload-access route、envelopeを変更していない。
- M14は承認後にtest-only crate-private quick runnerだけを追加し、既存`core_workflows` 10 scenarioを変更しない。
  full runnerはM36まで実装しない。公開API／C ABI／Windows／product routeは追加していないため、M13は`[~]`で停止する。

```text
Version impact:
- Registry schema: 2（proposal／測定記録のみ、schema変更なし）
- InkScript file: 1（grammar／serialized field／selector意味変更なし）
- InkScript procedure catalog: 1（private draft 13/84、entry／signature変更なし）
- replay epoch: 23（canonical invocation／state／pixel semantics変更なし）
- .inkpod top-level: 26（schema／encoder／decoder／replay変更なし）
- C ABI: 14（symbol／record変更なし）
```

### [ ] M14 — approved quick benchmark導入

**範囲**

- M13で承認されたworkload、counter、checksum、harness、envelopeだけを実装する。
- quick benchmarkを以後の該当milestoneで実行可能にする。

**完了条件**

- warm-up後の全sampleと中央値、semantic counter、environmentを記録する。
- 承認内容を変更・緩和せず、既存core_workflows benchmarkに回帰がない。

### [ ] M15 — catalog implementation A: document tree

**範囲**

- owner manifestのpaper/frame、layer/plane create/duplicate/delete/reorder/property/merge、delete-hidden、
  edit target exact entryをcatalog v1 draftへ追加して実装する。M07のvisibility/conversion entryは再利用する。
- typed result roleと後続step参照を実装する。
- strict ID binding、semantic selector、source UUID preconditionを各commandで検証する。

**完了条件**

- 対象familyの全journal-replayable primitiveに`ScriptStep <-> CanonicalInvocation` codec/execute
  equivalence testがある。journal fragment exportはM24より前に実装しない。
- create後参照、no-op result、ID high-watermark、Undo/Redo round-tripを検証する。
- 対象外familyへ先行実装しない。

### [ ] M16 — catalog implementation B: metadata、color、guide

**範囲**

- owner manifestのmain-line color、palette、color chart、document metadata、guide/grid exact entryを
  catalog v1 draftへ追加して実装する。
- 対応assert/selectorとinvocation依存portabilityを接続する。

**完了条件**

- exact-depth color、metadata no-op、guide order、strict/rebound preconditionのgolden testがある。
- `SetMainLineColor`、`ReplacePalette`、`ReplaceColorChart`をowner manifestどおり漏れなく扱う。
- M07/M08/M15 entryを再登録しない。

### [ ] M17 — catalog implementation C: stroke、import、geometry

**範囲**

- owner manifestのraster stroke、canonical raster import、geometry、gesture geometry依存gradient exact entryを
  catalog v1 draftへ追加して実装する。
- sample/rasterのinline-or-asset表現、sample order、exact numeric conversionを接続する。

**完了条件**

- native depth、Q16、sample order、asset roleのgolden testがある。
- direct operationとscript operationのcanonical arguments/payload/state digestが一致する。
- cancel/overflow/allocation failureがall-or-nothingである。

### [ ] M18A — catalog implementation D1: fill、gradient

**範囲**

- owner manifestのうちM08で未実装のfill/gradient exact entryをcatalog v1 draftへ追加して実装し、M08の
  Batch fill/filter entryは再利用する。
- selection/tile boundary、native depth、Q16、payload、work formula、cancellation boundary、state-coupled
  portabilityを接続する。

**完了条件**

- selection/tile boundary、native depth、Q16、payload、all-or-nothingのgolden/property testがある。
- direct operationとscript operationのcanonical invocation/state digestが一致する。
- M08 entryに重複ownerがない。

### [ ] M18B — catalog implementation D2: gesture effect、alpha、adjustment

**範囲**

- owner manifestのairbrush/stamp/blur gesture、alpha、adjustment、scoped color exact entryをcatalog v1 draftへ
  追加して実装し、M08のBatch filter/effect entryは再利用する。
- asset/payload role、alpha/native-depth、work formula、cancellation boundary、state-coupled portabilityを接続する。

**完了条件**

- gesture order、selection、alpha、native depth、payload、cancel/all-or-nothingのgolden/property testがある。
- direct operationとscript operationのcanonical invocation/state digestが一致する。
- M08/M18A entryに重複ownerがない。

### [ ] M19 — catalog implementation E: selection、floating、transform

**範囲**

- owner manifestのselection全family、floating commit、M07未実装document transformのexact entryを
  catalog v1 draftへ追加して実装する。
- list/scalar result、asset、strict precondition、result availabilityを接続する。

**完了条件**

- selection bounds、floating asset、no-op result、cancel/overflowのtestがある。
- mirror/rotate/resize等のM07 entryを再登録せずowner manifestと一致する。
- direct/scriptのCommit、Undo/Redo、state digest、ID high-watermarkが一致する。

### [ ] M20 — catalog implementation F: vector

**範囲**

- owner manifestのvector path/fill/erase/connect/width、rasterize/vectorize/new-layer exact entryを
  catalog v1 draftへ追加して実装する。
- 複数output IDをtyped list/role/ordinalへ対応させる。

**完了条件**

- path/fill複数result、asset、native depth、strict/reboundのequivalence testがある。
- result ordinalが全`output_ids`を重複なく覆い、後続index参照が動作する。
- direct/scriptのcache-free replayとstate digestが一致する。

### [ ] M21 — catalog implementation G: annotation、frame、vanishing point

**範囲**

- owner manifestのannotation、shooting frame、vanishing point exact entryをcatalog v1 draftへ追加して実装する。
- 可変長create/update/delete resultとinvocation依存portabilityを接続する。

**完了条件**

- 0/1/N output ID、list index、owner role、no-op availabilityのtestがある。
- exact-source/rebound export準備とcache-free replayがdirect routeに一致する。
- ID allocation digestとhigh-watermarkを検証する。

### [ ] M22 — catalog implementation H: Light Table

**範囲**

- owner manifestのjournal-replayable Light Table set/item exact entryをcatalog v1 draftへ追加して実装する。
- session-only swap、query、preview entryを明示除外する。

**完了条件**

- add/remove/reorder/propertyの複数result、asset retention、Undo/Redo testがある。
- non-replayable commandを誤登録せず、owner manifestと一致する。
- direct/script/cache-free replayが一致する。

### [ ] M23 — catalog v1 completenessとgenerated reference gate

**範囲**

- current journal-replayable primitiveとdraft entry、実装、owner、equivalence testの全単射を検査する。
- portability evaluator、result、asset、work、editor metadata未指定をbuild/test failureにする。
- validated draftを初めて`catalog-v1.json`としてfreezeし、language/catalog registryから
  `docs/inkscript-command-reference.md`を生成するtoolとdrift testを追加する。
- completeness gate成功後にだけInkScript compile/bind/runをRust public APIへre-exportし、catalog v1を
  production contractとして有効化する。FFI/Windows公開は後続milestoneで行う。

**完了条件**

- 未対応、重複、未実装、Debug由来名、reference driftがない。
- 全familyの`ScriptStep <-> CanonicalInvocation` codec/execute equivalenceとcache-free replayが通る。
- query/view/export/session commandが誤登録されていない。
- freeze後のcatalog変更がversion更新と旧version拒否testなしには通らない。

### [ ] M24 — journal-to-fragment exporter

**範囲**

- exact canonical procedure/runtime invocation/assets/state linkageからfragment ASTを作るCore APIを実装する。
- 一行export、連続線形祖先列、typed output role/index参照、external strict binding、parent-state
  preconditionを実装する。
- branch横断、非連続state、strict-only、oversize、cancelの診断を実装する。
- 可視化の表示用summary/thumbnailとexport authorityを分離する。

**完了条件**

- active/inactive branchを含む選択のpositive/negative testがある。
- source Coreを最初の選択Commitのparent stateまでcache-free replayしてfragmentを適用し、最終state/pixel
  digest、ID high-watermark、typed result role、各Commitのpre/post digest、schema role順のinput/output/asset
  ID、canonical procedure列が元選択列と一致する。
- Genesis直後からの選択だけはGenesis単独からも一致する。
- export queryがlive document/revision/history/dirty/savepoint/IDを変更しない。

### [ ] M25 — source/compiler/export C ABI

**範囲**

- source parse、diagnostic copy、summary、compile、fragment export/releaseのopaque handle APIを追加する。
- versioned size-prefixed record、bounded span、二段階UTF-8 copy、ownership/thread規則を文書化する。
- C headerとRust declaration drift test、C11/C++20 include testを更新する。
- C ABI exact-current versionを一つ上げ、旧versionを拒否する。

**完了条件**

- NULL、alignment、short struct、unknown flags/enums、oversize、double releaseのnegative testがある。
- panicがABIを越えず、error textが共有global mutable bufferを使わない。
- per-token/per-node FFI往復がなく、batch/span queryになっている。
- controller/owner thread、parent lifetime、release thread、generation invalidationがheaderと`docs/ffi.md`で一致する。

### [ ] M26 — execution/report C ABI

**範囲**

- immutable plan、input/output preview、run options、task、progress、reportのABIを追加する。
- PathIntent/authority/confirmation token、PlanTask/RunTask、cancel/releaseをbounded ABIへ追加する。
- C ABI exact-current versionを再度一つ上げ、旧versionを拒否する。

**完了条件**

- NULL、short record、unknown flag、queue saturation、cancel、stale plan/token、save failureのnegative testがある。
- task/report ownership、thread、release、shutdown中lifetimeがheaderとRustで一致する。
- per-item reportをspan単位でcopyし、callback中にCore lockを保持しない。

### [ ] M27A — Windows authority/file-identity adapter

**範囲**

- Windowsのhandle-based final-path/reparse、file identity、open-session registry、authority generation adapterを
  M11のOS非依存DTOへ接続する。Coreへ`HANDLE`やopaque token内部を渡さない。
- missing destination componentを検証済みparent handleから一componentずつno-followで処理し、
  create/replaceも検証済みparent handle相対にする。

**完了条件**

- 実file replacement/reparse/alias、missing中間componentのswap/reparse race、authority generation変化のtestがある。
- same-identity external writeをguard中に排他または検出し、tempを検証済みparent handle外へ作らず、
  temp name collision/reparse/authority失効/writer-close後identity swapで別objectをcleanup/installしないtestがある。
- open-session backing pathのaliasを含むoverwriteを拒否し、別file/sessionをread/replaceしない。
- Rust Coreと公開DTOに`HANDLE`、Windows path token、reparse固有型が入らない。

### [ ] M27B — Windows Core engine routeとprivate smoke

**範囲**

- `CommandContext`を固定し、Core engine threadでcompile/snapshot/bind/run/exportするrouteを作る。
- UIへvalue notificationだけを返し、private `--abi-smoke-test` / `--smoke-test`へproduction pathを追加する。
- 通常Batch paneのcommandはまだ切り替えない。

**完了条件**

- queue saturation、close中task、stale session、save failure、shutdown race testがある。
- UI threadがCore execution、PlanTask、Presentを同期waitせず、wait_ms中もengineをblockしない。
- smokeが実parser、catalog、executor、native writerを通る。

### [ ] M28A — private controller、source edit、file lifecycle

**範囲**

- production Batch pane、model、command registrationを変更しない。pane registryへ登録しないprivate
  `ScriptController` / `PaneHarness`を作る。
- lossless CSTの局所source edit API、diagnostic表示、`.inkscript` new/open/save-as/dirty/atomic saveをprivate
  harnessへ接続する。
- M28B～M32も同じharnessを拡張し、production切替はM34で一回だけ行う。

**完了条件**

- 局所編集、保存、再読込でcomment、BOM/CRLF、無関係source rangeを保持するtestがある。
- invalid/Cancel/save failureがsource、dirty、destinationを部分変更しない。
- production Batch paneが旧model一つだけを使い、private harnessとの二重正本を持たない。

### [ ] M28B — private source projectionとstructured editor

**範囲**

- source/AST projection、group/step editor、typed parameter editor、diagnostic navigationをM28A harnessへ接続する。
- multiple setの追加、削除、rename、reorder、enableをsource/file正本で実装する。

**完了条件**

- UIから作成、保存、再読込、group/step/parameter/set編集を行い、sourceとのround-tripが一致する。
- private x64 Release smokeと、既存Batch paneの回帰確認手順を記載する。
- production Batch pane、model、command registrationを変更しない。

### [ ] M29A — private preview/run UI

**範囲**

- input/output preview、Run current/all、dry-run、confirmation、progress、cancel、failure reportをprivate
  harnessへ接続する。
- continue/stop、wait、installed/failed/cancelled/not-started、save failureを表示する。

**完了条件**

- current/all、dry-run、preview confirmation、cancel、failure policy、reportのprivate smokeがある。
- owner-thread、close中job、save failureでstale別sessionへ通知または実行しない。
- production filter/commandはまだ`.inkbatch`から切り替えない。

### [ ] M29B — advanced Batch authoring UI

**範囲**

- seed consistency preview、two-cell exact-depth pair抽出、one-to-many ambiguity、native-file separation、
  transient each-run parameter UIをprivate harnessへ接続する。

**完了条件**

- multi-row enable/order、seed drift、exact-depth/alpha pair、ambiguity resolve/exclude、typed separationを検証する。
- each-run Cancel/invalid/unresolvedでjob/source/defaultを変更しない。
- M08/M09のtyped model以外の第二実装をWindowsへ作らない。

### [ ] M29C — shadow parityとprivate UI gate

**範囲**

- 旧BatchGraphとInkScriptを同じfixtureでshadow実行し、16.4の全parity項目を比較する。
- Japanese/English、keyboard、DPI/high contrast、owner-thread、cancel/save failureをprivate routeで確認する。
- `.inkbatch` importerを作らない。

**完了条件**

- `BATCH-001..004`の追跡表とshadow evidenceがすべてgreenである。
- private x64 Release smokeとaccessibility evidenceが揃う。
- production filter/commandはまだ`.inkbatch`から切り替えない。

### [ ] M30 — Batch fragment clipboard

**範囲**

- Batch step/rangeはfragment挿入paste、full scriptは明示的な全体置換pasteとしてcopy/pasteを実装する。
- `Inkpod.InkScript.v1`と`CF_UNICODETEXT`、dependency closure、asset dedup、name collisionを実装する。
- group copy、canonical text、one UI transaction、oversize asset拒否を実装する。

**完了条件**

- Batch→Batch、plain-text editor→Batchのfragment挿入/full-file置換round-trip smokeがある。
- full fileをfragmentへ暗黙変換せず、通常pasteでinputs/output/executionをmergeまたは破棄しない。
- dependency closure、range外producer、large asset、clipboard ownership、name rewriteを検証する。
- pasteが一回のUI model transactionで、Cancel/error時にsource/jobを変更しない。

### [ ] M31 — History fragment clipboard

**範囲**

- 履歴可視化のCommit一行/連続線形列からM24 exporterを使ってexact fragmentをcopyする。
- snapshot authority、branch/noncontiguous診断、typed result、asset closureを接続する。
- summary/thumbnail/localized labelをexecutable textへ流用しない。

**完了条件**

- active/inactive branchのpositive/negative、session close race、oversize、cancel testがある。
- History→Batchでexact preconditionを保持し、source stateを変更しない。
- `CF_UNICODETEXT`も同じcanonical Unicode textを返す。

### [ ] M32 — strict binding rebind UI

**範囲**

- kind/name/owner/format hintから候補を表示し、semantic selectorへの明示rebindを実装する。
- 0件、複数件、stale candidate、Cancelを拒否/無変更にする。
- `strict_source_only` invocationはrebind不能理由を表示する。
- 全strict selector置換後にportability/preconditionを再評価し、利用者確認付きでexact-source三fieldを除去して
  rebound fragmentを再compileする。command固有preconditionを無言で緩和しない。

**完了条件**

- History→別document、Batch→Batch、list result、large assetのround-trip smokeがある。
- active objectや同名objectへ無言で再解決しない。
- strict残存時の拒否、exact assert除去、保証降格表示を検証する。
- paste/rebindが一回のUI model transactionで、Cancel/error時にsource/Undo/jobを変更しない。

### [ ] M33A — malformed-input fuzz corpus

**範囲**

- parser/formatter/catalog/asset/selector/runner/clipboardのfuzz corpusを完成させる。
- parser recovery、container/list/reference上限、catalog formula、asset descriptor、clipboard fragment境界を
  malformed inputで継続検査する。
- fuzz由来の安全性修正だけを扱い、fault injectionやbenchmark workload/envelope変更を混ぜない。

**完了条件**

- corpusがpanic、OOM、unbounded recovery、上限回避を起こさない。
- crash/minimized corpusをregression testとして固定し、同じ入力がbounded diagnosticまたは明示拒否になる。
- 修正範囲のRust testとfuzz smokeがgreenである。

### [ ] M33B — fault/path-race hardening

**範囲**

- allocation failure、queue saturation、shutdown、confirmation replay、file replacement、reparse/path raceを
  fault injectionする。
- item staging、shared directory graph、atomic installの各barrierでcancel/failure/raceを再現する。
- benchmark workload/envelopeを変更せず、承認済みquick scenarioだけを回帰gateに使う。

**完了条件**

- fault注入がpanic、resource leak、不正なpartial installを起こさず、report outcomeと実destinationが一致する。
- stale/alias/authority raceで別file/sessionをread/replaceしない。
- 修正範囲のRust/Windows testとapproved quick benchmarkがgreenである。

### [ ] M34 — production cutover、公開ABI廃止、仕様/docs同期

**範囲**

- 通常Batch paneのowner/model/command registrationを、M28A～M32で完成した`ScriptController`へ一回で置換し、
  resource/accessibility、open/save/run/clipboardを`.inkscript`へ切り替える。
- `.inkbatch`のpublic open/save/run C ABI symbol、export、file filterを同じcutoverで削除し、C ABI versionを
  一つ上げ、header、FFI docs、C11/C++20 include、旧version拒否smokeを更新する。
- `SPEC.md`をcurrent InkScript契約へ更新し、BATCH履歴に`Superseded by SCRIPT-*`の対応を記録する。
- README、architecture、file-format、compatibility、implementation-statusを同じ変更で更新する。
- M29Cのshadow結果から、legacy実装へ依存しないcanonical invocation/state/ID/report goldenを固定する。
- 旧`.inkbatch`実装はproductionから到達不能なtest-private comparatorとしてM35までだけ残してよい。

**完了条件**

- user-facing正本、通常command、file filter、公開ABIが`.inkscript`へ一本化され、`.inkbatch`を安全に拒否する。
- M29C～M32のparity/clipboard/rebind evidenceとM33A～M33Bのfuzz/fault/path-race gateを
  production routeで再検証する。
- 削除済みsymbol、旧ABI version、`.inkbatch` extension/magicのnegative smokeがあり、二つの公開契約を残さない。
- nonlegacy goldenが旧comparatorなしで読め、`SCRIPT-*`は利用者確認前に`Verified`へしない。

### [ ] M35 — `.inkbatch` test-private実装の完全削除

**範囲**

- test-private `.inkbatch` parser/writer、BatchGraph persistence、旧runner/model、legacy専用fixtureを削除する。
- 再利用algorithmだけをscript ownerへ移し、二重model/executorとproductionからlegacy ownerへの参照を残さない。
- compatibilityのBATCH履歴、廃止理由、`.inkbatch` extension/magic rejection test、M34のnonlegacy parity goldenは残す。
- M34で廃止済みのpublic symbol/filterやC ABI version更新をこのmilestoneへ遅延させない。

**完了条件**

- production/test support sourceに旧reader/writer/model/runner implementationがなく、literal全消去に依存しない
  owner/target/drift gateがある。
- `.inkbatch`をunsupported extension/formatとして安全に拒否し、import/migration/shimを行わない。
- nonlegacy parity goldenが旧codeなしで通り、reusable algorithmに旧Batch ownershipがなく、
  Rust/Windows regressionがgreenである。

### [ ] M36 — approved full benchmarkとperformance gate

**範囲**

- M13で承認済みのfull scenarioだけを実行・接続する。
- large source/asset、1,000+step、multi-item、cancel/failure、cache-free replayを意味counter付きで測定する。
- workload、harness、counter、envelopeを変更しない。

**完了条件**

- 同一machine/profile/inputのwarm-up後全sample、中央値、semantic counter、environmentを記録する。
- approved envelopeを無断緩和せず、独立再測定で再現する回帰を完了扱いしない。
- 既存core_workflowsとInkScript quick/fullのchecksum/counterが一致する。

### [ ] M37 — Windows hardeningと最終evidence

**範囲**

- soak、IME、DPI、high contrast、screen reader、keyboard、device reset中progressを確認する。
- `AGENTS.md`記載のRust/Windows検証一式を実行する。
- compatibility/statusを最終evidenceと既知差分で更新する。

**完了条件**

- Rust/Windows build、CTest、smoke、ABI、fuzz regression、approved benchmarkがgreenである。
- shutdown/close/device reset中もCore、task、snapshot、clipboard ownership違反がない。
- 利用者受入後だけ`SCRIPT-*`を`Verified`にし、未検証事項を明記する。

## 18. プロンプト例

以下を各セッションの開始時にそのまま使用する。

```text
AGENTS.md、SPEC.md、INKSCRIPT.md、git status、既存差分、対象コードとテスト、関連する
implementation-status / compatibilityを確認してください。MACOS.mdとその内容は無視してください。

最初にINKSCRIPT.mdのマイルストーン状態を確認してください。`[!]`があれば最初の一件だけを修正し、
次へ進まないでください。直前が`[~]`で、私が不具合や確認失敗を併記していない場合、このpromptの送信は
前回の最終報告に記載された利用者確認または承認を完了し、問題がなかったという報告を兼ねます。
その`[~]`を`[x]`へ更新してから、最初の`[ ]`のマイルストーンをこのsessionで一つだけ実装してください。
不具合を併記した場合は直前を`[!]`にして同じmilestoneだけを修正してください。marker列が
`[x]* ([~]または[!])? [ ]*`になっていない場合は、先へ進まず状態不整合を報告してください。

未完了の先頭が明示承認を必要とするgateの場合は、判断材料を提示して`[~]`で停止し、承認なしに実装や
次のmilestoneへ進まないでください。今回扱うmilestone以外の機能、catalog entry、UI、refactorを
先行実装しないでください。placeholder、stub、二重model、二重executorを作らず、ユーザー変更を
保護してください。

公開契約をtestで先に固定し、success、no-op、invalid、cancel、stale、overflow、atomicity、resource、
ownership/thread規則のうち該当するものを検証してください。native outputを扱う場合はsave/reopen、
Undo/Redo、cache-free replay、ID high-watermark、document/editor savepointも検証してください。

各変更でInkScript file version、procedure catalog version、replay epoch、.inkpod top-level version、
C ABI versionへの影響を明示的に判定してください。serialized grammar/catalog/ABIを変更する場合は
exact-current versionを同じ変更で更新し、旧version拒否testを追加してください。canonical replay
semantics、M00で批准したlanguage core、またはM23で批准したcatalog v1を変更する必要が生じた場合は
推測で進めず、作業を止めて影響、選択肢、必要なversion bumpを説明してください。M23以前にowner
milestoneがprivate catalog draftへexact entryを追加すること自体はversion bumpにせず、draftをproductへ
公開しないgateを維持してください。

変更範囲に応じてformat、clippy、test、rustdoc、承認済みquick benchmark、Windows configure/build/
CTest/smokeを実行してください。benchmark workload、harness、semantic counter、承認済みenvelopeを
変更・緩和しないでください。実行できない検証は隠さず報告してください。

自動検証まで完了し、利用者確認が必要なmilestoneは`[x]`ではなく`[~]`にしてください。手動確認も
承認も不要なdocs-only milestoneだけは完了条件を満たせば`[x]`にできます。testと必要な利用者確認なしに
compatibilityを`Verified`へしないでください。

一つのmilestoneを終えたら次へ進まず停止してください。commit、push、PRは行わないでください。
最終報告には、利用者向け挙動、重要な設計判断、変更file、version impact、実行した検証と結果、
未検証事項、既知差分、私がbinaryで確認すべき具体的手順を簡潔に記載してください。
```
