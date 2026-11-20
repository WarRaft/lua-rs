# lua-rs

A parser, linter and semantic highlighter for **Lua 5.3**, the scripting
language Warcraft III Reforged runs map scripts in. It's built like the
other language crates, on the shared [`parser-rs`](../parser-rs).

- [The Lua language](#the-lua-language): a syntax reference to build on
- [Parser status](#parser-status): what `lua-rs` handles today
- [CLI](#cli) · [Tests](#tests)

How the parser is built, and the rules every parser here follows, is in
the [`parser-rs` README](../parser-rs/README.md#principles).

---

## The Lua language

Lua is dynamically typed, lexically scoped and expression-oriented. A file
is a *chunk*: a block of statements, run as an anonymous vararg function.
In a WC3 map, `war3map.lua` is one chunk, the game's natives (`CreateUnit`,
`TimerStart`, ...) are plain global functions, and `FourCC('hfoo')` takes
the place of JASS rawcodes.

### Lexical structure

| element     | syntax                                                                                   |
|-------------|------------------------------------------------------------------------------------------|
| comments    | `-- to end of line`, long comments `--[[ ... ]]`, `--[==[ ... ]==]` (any number of `=`)  |
| statements  | separated by whitespace; `;` is optional                                                 |
| names       | letters, digits, `_`; not starting with a digit; case-sensitive                          |
| integers    | `42`, `0xFF` (wraps to 64 bits)                                                          |
| floats      | `3.5`, `.5`, `1e10`, `2.5E-3` (also hex floats `0x1p4`, not yet lexed here)              |
| strings     | `"..."` / `'...'` with escapes `\n \t \\ \" \' \xHH \ddd \u{XXX} \z`; long `[[...]]`, `[==[...]==]` |
| constants   | `nil`, `true`, `false`, `...` (varargs)                                                  |

**Keywords:** `and break do else elseif end false for function goto if in
local nil not or repeat return then true until while`.

### Values and types

`nil`, boolean, number (64-bit integer or double subtypes), string,
function, table, userdata (the game's handles) and thread. Only `nil` and
`false` are falsy. Tables are the only data structure: arrays (1-based),
maps, objects, and modules via metatables (`setmetatable`, `__index`).

### Statements

| statement          | syntax                                                                                          |
|--------------------|-------------------------------------------------------------------------------------------------|
| local              | `local a, b <const> = 1, 2` (`<const>`/`<close>` attributes are Lua 5.4, accepted here)        |
| assignment         | `a, t.x, t[k] = 1, 2, 3` (multiple assignment; all right-hand sides are evaluated first)       |
| call               | `f(x)`, `obj:method(x)`, `f "str"`, `f { table }`                                               |
| block              | `do ... end`                                                                                    |
| conditional        | `if c then ... elseif c then ... else ... end`                                                  |
| loops              | `while c do ... end`, `repeat ... until c` (`until` sees the body's locals)                     |
| numeric for        | `for i = start, stop [, step] do ... end`                                                       |
| generic for        | `for k, v in pairs(t) do ... end`                                                               |
| functions          | `function a.b.c(x) end`, `function obj:m(x) end` (implicit `self`), `local function f() end`   |
| jumps              | `break`, `goto name`, `::name::`                                                                |
| return             | `return [values]`, only as the **last** statement of a block                                    |

### Expressions

| precedence (low → high) | operators                                    | associativity |
|-------------------------|----------------------------------------------|---------------|
| 1                       | `or`                                         | left          |
| 2                       | `and`                                        | left          |
| 3                       | `<` `>` `<=` `>=` `~=` `==`                  | left          |
| 4                       | `\|` (bitwise or)                            | left          |
| 5                       | `~` (bitwise xor)                            | left          |
| 6                       | `&`                                          | left          |
| 7                       | `<<` `>>`                                    | left          |
| 8                       | `..` (concatenation)                         | **right**     |
| 9                       | `+` `-`                                      | left          |
| 10                      | `*` `/` `//` (floor division) `%`            | left          |
| 11                      | unary `not` `#` `-` `~`                      | n/a           |
| 12                      | `^` (power)                                  | **right**     |

- **`and`/`or` precedence.** Unlike JASS and Zinc, Lua's `and` binds
  *tighter* than `or`, as in C.
- **`^` and unary operators.** `^` binds tighter than unary operators on
  its left, so `-x^2` is `-(x^2)`. `not a == b` is `(not a) == b`.
- **Results.** `and`/`or` short-circuit and return an operand, not a
  boolean: `x = x or 0`.
- **Operands.** Literals, `...`, names, `t.k`, `t[k]`, calls, method
  calls, `function (params) ... end`, table constructors
  `{ v, k = v, [k] = v }` (with `,` or `;` as separator), and
  parenthesized expressions. `(f())` truncates a call to its first
  result.

### Scoping

A local is visible from the statement *after* its declaration to the end
of its block, so `local x = x` reads the outer `x`. A `local function f`
is visible inside its own body; parameters and loop variables are local
to the body. A `goto` may only jump to a label visible in an enclosing
block of the same function, never into a nested block.

### Example

```lua
local Counter = { created = 0 }
Counter.__index = Counter

function Counter.new(start)
    local self = setmetatable({ value = start or 0 }, Counter)
    Counter.created = Counter.created + 1
    return self
end

function Counter:bump(by)
    self.value = self.value + (by or 1)
    return self.value
end

for i, c in ipairs({ Counter.new(1), Counter.new(2) }) do
    print(i, c:bump())
end
```

---

## Parser status

Supported: everything in the reference above. Not yet:

- hex float literals (`0x1p4`);
- the integer/float distinction for decimal literals: `3` is an integer
  and `3.0` a float, as in Lua, but overflowing decimal integers are an
  error rather than turning into floats.

Every lint diagnostic names its rule. The rule ids, which rules are on
by default (`infinite-loop` isn't) and how a project switches them
(`[lint]` in `wc3.bni`) are in the [`wc3-rs` README](../wc3-rs/README.md#lint).

Lint rules:

- **Errors:**
  - `break` outside a loop (a function body doesn't count as inside the
    loop around it);
  - `goto` with no visible label, and duplicate labels;
  - `...` outside a vararg function.
- **Warnings:**
  - unused local variables and local functions;
  - `while true` with no `break`/`return`/`goto`;
  - empty `then` blocks.

Unused-local detection follows Lua's real scoping. Parameters, loop
variables and names starting with `_` are never reported.

Semantic highlighting:

- **Declarations:** locals, `local function`s, parameters (including the
  implicit `self` of `function a:m()`), loop variables, `<const>`
  attributes (`decorator`) and labels.
- **References:** globals, plus standard-library globals such as `print`,
  `pairs`, `string` and `math`, which get `defaultLibrary`.
- **Members:** fields and field functions (`t.f()`), methods (`obj:m()`)
  and table keys.
- **Assignments** get the `modification` modifier.

It runs on the tolerant token stream, so broken code still highlights.

**Symbols** (`symbols`, `semantic_tokens_and_symbols`): every name as a
`parser_rs::Occurrence`, recorded by the highlighting classifier in the
same pass, so they always agree.

| names                                                         | scope |
|---------------------------------------------------------------|-------|
| locals, parameters (and `self`), loop variables, `local function`s | **each declaration is its own binding** (`Local(n)`); a use refers to the binding Lua's scoping finds, so `local x = x + 1` is two symbols |
| globals                                                       | project: a Warcraft III map's Lua files share one global table, and a JASS `native` of the same name is the same symbol |
| fields and methods: `t.x`, `t:m()`, `{ x = 1 }`, `function t.f()` | `Member`: matched by name, since which table a name belongs to isn't known without types |

What counts as a declaration: a `local`, parameter or loop variable;
`function Foo()` and a top-level assignment `Foo = ...` for a global (an
assignment inside a function or block is a write); `function t.f()`,
`function t:m()`, a table key `{ x = ... }` and a top-level `t.x = ...`
for a field. `function foo()` with a local `foo` in scope assigns that
local. Labels aren't symbols.

## CLI

```sh
cargo run -- file.lua            # lint
cargo run -- --ast file.lua      # indented AST tree
cargo run -- --dot file.lua      # Graphviz DOT
cargo run -- --html file.lua > file.ast.html && open file.ast.html
```

## Tests

```sh
cargo test
UPDATE_SNAPSHOTS=1 cargo test --test html_snapshot   # regenerate HTML goldens
```
