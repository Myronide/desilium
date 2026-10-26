# Pureline Specification 1.5

## Definition

**Pureline** is an opinionated coding style focused on linear control flow, explicit intent, compact formatting, small units, clear ownership, and minimal syntactic or architectural noise.

Pureline does not aim to minimize characters. It aims to minimize **unnecessary code**.

The preferred reading direction is:

```text
Input
↓
Validation
↓
Transformation
↓
Execution
↓
Result
```

Not:

```text
if
 └─ if
     └─ else
         └─ if
             └─ callback
                 └─ branch
```

The central Pureline principles are:

> **Linear Flow. Explicit Intent. Minimal Noise. Small Units. Clear Ownership. Cohesion over Size.**

**Version 1.5:** Adds the idiomatic Rust profile (`PL-RS-001` through `PL-RS-020`) while preserving all existing rule IDs and the 1.4 Cohesion over Size principle.

---

# 1. Pureline Core

These rules apply to all supported languages where the language allows them naturally.

| ID | Rule |
|---|---|
| PL-CORE-001 | Control flow should be as linear as possible |
| PL-CORE-002 | Avoid `else` |
| PL-CORE-003 | Prefer guard clauses |
| PL-CORE-004 | Use the clearest control-flow form for the language; brace-based languages use explicit blocks |
| PL-CORE-005 | Idiomatic language-native shorthand is allowed when it remains immediately readable |
| PL-CORE-006 | Functions should be small and focused |
| PL-CORE-007 | Names must carry meaning |
| PL-CORE-008 | Avoid unnecessary abstractions |
| PL-CORE-009 | Avoid God Classes and God Modules |
| PL-CORE-010 | Prefer immutable state |
| PL-CORE-011 | Prefer modern language features where they improve clarity |
| PL-CORE-012 | Avoid unnecessary vertical formatting |
| PL-CORE-013 | Comments must not compensate for unclear code |
| PL-CORE-014 | Handle invalid and error states early |
| PL-CORE-015 | Keep important side effects visible |
| PL-CORE-016 | Resources must have a clear lifecycle |
| PL-CORE-017 | Prefer framework-native APIs |
| PL-CORE-018 | Avoid magic values |
| PL-CORE-019 | Make dependencies explicit |
| PL-CORE-020 | Optimize hot paths deliberately, not accidentally |
| PL-CORE-021 | Keep packages and source folders small enough to scan at a glance |
| PL-CORE-022 | Cohesion over Size: prefer one coherent responsibility over arbitrary class line limits |

---

# 2. Control Flow

Control flow is one of the most important parts of Pureline.

Pureline standardizes **clarity and structure, not identical syntax across languages**. Java, JavaScript, TypeScript, Go, Emerald, and Emvus use explicit blocks for control flow. Languages with established readable shorthand, such as Crystal postfix guards, may use that shorthand when the condition and action remain simple.

## PL-CF-001 — Prefer Guard Clauses

Preferred:

```java
if (request == null) {
    return Result.empty();
}

if (!request.valid()) {
    return Result.invalid();
}

var session = sessions.create(request.user());
return executor.execute(session, request);
```

Avoid:

```java
if (request != null) {
    if (request.valid()) {
        var session = sessions.create(request.user());
        return executor.execute(session, request);
    } else {
        return Result.invalid();
    }
}

return Result.empty();
```

The happy path should remain as close to the base indentation level as possible.

## PL-CF-002 — Avoid `else`

Avoid:

```java
if (valid) {
    execute();
} else {
    reject();
}
```

Prefer:

```java
if (!valid) {
    reject();
    return;
}

execute();
```

A ternary expression may be used for simple value selection:

```java
var state = valid ? State.READY : State.INVALID;
```

Complex nested ternaries are not Pureline.

## PL-CF-003 — Explicit Blocks in Brace-Based Languages

In Java, JavaScript, TypeScript, Go, and other brace-based languages, control-flow bodies use explicit blocks.

Not Pureline:

```java
if (!valid) return;
```

Also not Pureline:

```java
if (!valid)
    return;
```

Pureline:

```java
if (!valid) {
    return;
}
```

The same principle applies to `for`, `while`, and comparable control-flow constructs.

This rule does **not** ban idiomatic shorthand in languages where that shorthand is a normal, readable part of the language. Crystal postfix guards such as `return unless valid` are Pureline-compliant when kept simple.

---

# 3. Naming

## PL-NAME-001 — Names Must Be Meaningful

Good:

```text
request
session
player
packet
handler
storage
result
connection
worker
```

Avoid:

```text
r
s
p
x
tmp
obj
data2
thing
```

Also avoid names that are unnecessarily long:

```text
currentIncomingPlayerConnectionRequestObject
```

The rule is:

> **As short as possible, as explicit as necessary.**

Exceptions are allowed for established local conventions such as loop counters:

```java
for (int i = 0; i < size; i++) {
}
```

---

# 4. Functions

## PL-FN-001 — One Clear Responsibility

Preferred names:

```text
validateRequest()
createSession()
sendPacket()
loadUser()
closeConnection()
```

Avoid vague names:

```text
handleEverything()
process()
doStuff()
executeAll()
```

A function should normally represent one clearly identifiable action.

### Size guidance

```text
~5–30 lines  → normal
~30–50 lines → review
50+ lines    → usually split
```

These are heuristics, not absolute limits.

---

# 5. Classes and Modules

## PL-STRUCT-001 — One Clear Responsibility

Avoid structures like:

```text
ServerManager
 ├─ database
 ├─ players
 ├─ packets
 ├─ commands
 ├─ configuration
 ├─ HTTP
 └─ logging
```

Prefer:

```text
server/
    ServerRuntime

network/
    Connection
    PacketRegistry
    PacketDispatcher

session/
    Session
    SessionService

storage/
    UserStorage
    SessionStorage
```

A class has one clear responsibility when its members and internal state contribute to one cohesive concern. **Line count alone does not determine whether a class is well designed.** See `PL-STRUCT-003` for size guidance.

# 5.1 Package and Folder Density

## PL-STRUCT-002 — Keep Namespaces Small Enough to Scan

Pureline treats file and type density as part of code readability.

A package or source folder should be small enough that a developer can understand its contents at a glance without searching through a long flat list of unrelated files.

When the limit is exceeded, split the package or folder by responsibility instead of allowing it to become a dumping ground.

### Default limits

| Language | Pureline default |
|---|---|
| Java | Maximum 8 production top-level types/source files per package |
| Emerald | Maximum 8 production `.ems` source files per source/namespace folder |
| Emvus | Maximum 8 production `.ev` source files per package/source folder |
| JavaScript | Maximum 10 production source modules per feature/source folder |
| TypeScript | Maximum 10 production source modules per feature/source folder |
| Go | Maximum 12 non-test, non-generated `.go` files per package directory |
| Crystal | Maximum 10 production `.cr` files per source/namespace folder |
| Rust | Maximum 10 hand-maintained production `.rs` files per feature/source folder |

These files do not count toward the limit:

- tests and specs
- generated source files
- metadata-only files
- `package-info.java`
- `module-info.java`
- JavaScript/TypeScript barrel files such as `index.ts` or `index.js`
- generated bindings or protocol output
- migration snapshots and comparable machine-generated artifacts

The purpose of this rule is not to force arbitrary folder nesting. A split should introduce a real responsibility boundary.

Bad:

```text
network/
    Connection.java
    ConnectionFactory.java
    ConnectionState.java
    Packet.java
    PacketCodec.java
    PacketDecoder.java
    PacketEncoder.java
    PacketRegistry.java
    PacketDispatcher.java
    PacketHandler.java
    PacketQueue.java
    PacketMetrics.java
    SocketConfig.java
    SocketRuntime.java
```

Preferred:

```text
network/
    Connection.java
    ConnectionFactory.java
    ConnectionState.java

network/packet/
    Packet.java
    PacketCodec.java
    PacketDecoder.java
    PacketEncoder.java
    PacketRegistry.java
    PacketDispatcher.java

network/runtime/
    PacketQueue.java
    PacketMetrics.java
    SocketConfig.java
    SocketRuntime.java
```

A project may override these thresholds through a future Pureline configuration file, but the default rule should remain intentionally strict.

---

# 5.2 Cohesion over Size

## PL-STRUCT-003 — Cohesion over Size (Zusammengehörigkeit vor Zeilenbegrenzung)

> **Cohesion over Size — Zusammengehörigkeit vor Zeilenbegrenzung.**

The preferred maximum class size is **guidance, never a hard limit**. Keep strongly related logic together when splitting it would make the code harder to read, maintain, test, or reason about. A larger cohesive class is preferable to multiple artificial classes connected by unnecessary delegation.

### Class size guidance

| Class size | Pureline recommendation |
|---|---|
| Up to 200 lines | Preferred normal case |
| 201–400 lines | Unproblematic when the class has a clear responsibility |
| 401–600 lines | Deliberate architectural decision; keep its cohesion understandable |
| More than 600 lines | Review recommended, **not prohibited** |

The ranges are **heuristics, not hard limits**. Crossing a threshold must not, by itself, produce a style violation or require a split.

### When larger classes are justified

- **Parsers and lexers:** A cohesive state machine may become more complicated when its states and transitions are scattered across artificial abstractions.
- **Algorithms:** Tightly coupled processing phases can be easier to understand and optimize when kept together.
- **Central data structures:** Methods sharing the same invariants and internal state can belong in one class.
- **Complex but cohesive logic:** Splitting is counterproductive when it adds indirection, unnecessary delegation, or more coordination than clarity.

### When to split anyway

Split by **real responsibility boundaries**, not by line count. Indicators include distinct subsystems evolving independently, unrelated reasons to change, weak coupling among groups of methods, or separate lifecycles that are obscured by one class.

**Tooling rule:** Pureline Checker and IDE inspections may show an informational size notice with a cohesion-review suggestion, particularly above 600 lines. They must not treat the number of lines as sufficient evidence of a violation, automatically extract classes, or require suppressions merely to keep a cohesive class intact.

This principle applies to **all Pureline language profiles**, including Java, JavaScript, TypeScript, Go, Crystal, Rust, Emerald, and Emvus. Language-specific syntax and idioms continue to take precedence.

---

# 6. Formatting

## PL-FMT-001 — Prefer Compact Horizontal Calls

Preferred:

```java
var box = Block.box(min, min, max, max, max, 16);
```

Avoid unnecessary vertical expansion:

```java
var box = Block.box(
    min,
    min,
    max,
    max,
    max,
    16
);
```

Vertical formatting is allowed when it reveals meaningful semantic structure.

Example:

```java
var server = Server.builder()
    .host(config.host())
    .port(config.port())
    .executor(executor)
    .build();
```

Pureline distinguishes between unnecessary vertical expansion and meaningful semantic grouping.

---

# 7. Comments

## PL-DOC-001 — Comments Explain Why, Not What

Avoid:

```java
// Check if the player exists
if (player == null) {
    return;
}
```

Useful comments explain things such as protocol constraints, workarounds, external bugs, non-obvious mathematical behavior, or unusual performance constraints.

Code should explain **what** happens. Comments should explain **why** something unusual is necessary.

---

# 8. Java Profile

## PL-JAVA-001 — Package by Responsibility

Preferred:

```text
src/main/java/
    space/example/project/
        api/
        runtime/
        service/
        network/
        storage/
        model/
```

Avoid generic dumping grounds such as `util/`, `manager/`, `misc/`, or `helper/` unless the package has a real and narrow purpose.

## PL-JAVA-002 — Naming

Classes:

```java
SessionService
PacketRegistry
UserStorage
ConnectionState
```

Interfaces use the `I` prefix:

```java
ISession
IStorage
IPacket
IConnection
```

Methods:

```java
open()
close()
load()
save()
execute()
resolve()
```

Variables:

```java
request
session
player
connection
```

Single-letter variables are disallowed unless they are established local conventions such as loop counters.

## PL-JAVA-003 — Prefer Records for Immutable Data

Preferred:

```java
public record SessionId(UUID value) {
}
```

Avoid boilerplate POJOs when a record is sufficient.

## PL-JAVA-004 — Explicit Null Handling

Avoid:

```java
Objects.requireNonNull(value);
```

for normal control-flow validation.

Prefer:

```java
if (value == null) {
    return;
}
```

or a domain-specific failure:

```java
if (config == null) {
    throw new ConfigurationException("Missing configuration");
}
```

## PL-JAVA-005 — Use Imports Instead of FQCNs

Avoid:

```java
java.util.concurrent.CompletableFuture<Result> future;
```

Prefer:

```java
import java.util.concurrent.CompletableFuture;
```

## PL-JAVA-006 — Prefer Modern Java

Use modern Java features when they improve the code:

```text
records
sealed types
pattern matching
switch expressions
var
virtual threads
text blocks
streams
```

Modern syntax should reduce noise, not create novelty for its own sake.

## PL-JAVA-007 — Prefer Switch for Real State Branching

Preferred:

```java
return switch (state) {
    case READY -> start();
    case CLOSED -> stop();
    case FAILED -> recover();
};
```

Prefer this over long chains of unrelated state checks.

## PL-JAVA-008 — Concurrency

Do not block main or tick threads with HTTP, database operations, filesystem access, or blocking network I/O.

Virtual threads are preferred for blocking I/O where appropriate.

## PL-JAVA-009 — Streams Only When Clearer

Streams are encouraged when they improve readability. Use a loop if it is clearer, easier to debug, or better suited to the hot path.

## PL-JAVA-010 — Lifecycle Symmetry

Anything that is opened, registered, started, allocated, or subscribed must have an equally clear corresponding close, unregister, stop, release, or unsubscribe path.

---

# 9. JavaScript Profile

## PL-JS-001 — Braces Are Mandatory

Preferred:

```js
if (!user) {
    return;
}
```

Not Pureline:

```js
if (!user) return;
```

## PL-JS-002 — Use Semicolons

Preferred:

```js
const session = createSession(user);
session.start();
```

Pureline JavaScript uses semicolons consistently.

## PL-JS-003 — `const` First

Preferred:

```js
const user = loadUser();
```

Use `let` only for actual reassignment:

```js
let count = 0;
count++;
```

Do not use `var`.

## PL-JS-004 — Prefer `async` / `await`

Preferred:

```js
async function loadUser(id) {
    const response = await fetch(`/users/${id}`);

    if (!response.ok) {
        return null;
    }

    return response.json();
}
```

Avoid promise chains when `async` / `await` produces clearer control flow.

## PL-JS-005 — Arrow Functions Are Contextual

Normal named logic may use regular functions:

```js
function createSession(user) {
    return new Session(user);
}
```

Arrow functions are well suited for local callbacks:

```js
users.filter(user => user.active());
```

Do not use arrow functions everywhere by default.

## PL-JS-006 — Destructuring Must Improve Clarity

Good:

```js
const { id, name } = user;
```

Avoid destructuring large objects only because the language allows it.

---

## PL-JS-007 — Folder Density

A JavaScript feature/source folder may contain at most **10 production source modules** by default.

Barrel files such as `index.js`, tests, generated files, and metadata-only files do not count.

If the folder exceeds the limit, split it by responsibility rather than by arbitrary file count.

---

# 10. TypeScript Profile

TypeScript inherits all Pureline JavaScript rules.

## PL-TS-001 — Avoid `any`

Avoid:

```ts
function load(value: any): any {
}
```

Prefer:

```ts
function load(value: UserId): User {
}
```

Use `unknown` for truly unknown data:

```ts
function parse(input: unknown): Config {
}
```

## PL-TS-002 — Use Types Deliberately

Both inferred and explicit local types are Pureline-compliant.

```ts
const user = loadUser();
```

is valid when the inferred type is obvious.

Explicit typing is equally valid:

```ts
const user: User = loadUser();
```

Pureline does **not** treat an explicit annotation as redundant merely because TypeScript could infer it.

Explicit types are especially useful when they:

- make intent clearer
- document an expected contract
- protect a boundary from unintended widening
- improve refactoring confidence
- make code easier to understand without following an entire call chain

Type inference should remove noise. It should never become a rule against using TypeScript's type system.

## PL-TS-003 — No Java-Style `I` Prefix

Preferred:

```ts
interface Session {
}
```

Not:

```ts
interface ISession {
}
```

The `I` prefix is profile-specific: Java, Emerald, and Emvus use it; TypeScript does not.

## PL-TS-004 — `interface` vs `type`

Use `interface` for object-like contracts:

```ts
interface User {
    id: string;
    name: string;
}
```

Use `type` for unions, aliases, and callable shapes:

```ts
type SessionState = "ready" | "closed" | "failed";
```

```ts
type Handler = (packet: Packet) => Promise<void>;
```

## PL-TS-005 — Prefer Union Types Over Enums When Appropriate

Preferred:

```ts
type State = "idle" | "ready" | "closed";
```

Use an enum only when an actual enum construct provides value.

## PL-TS-006 — Strong Types at Boundaries, Freedom Internally

Public APIs, external data, persistence boundaries, and network payloads should normally expose clear types.

Inside an implementation, both explicit annotations and inference are valid. Choose the form that makes the code easiest to understand. Pureline does not score one as cleaner purely because it contains fewer type annotations.

---

## PL-TS-007 — Folder Density

A TypeScript feature/source folder may contain at most **10 production source modules** by default.

Barrel files such as `index.ts`, declaration-only support files, tests, generated files, and metadata-only files do not count.

A split should represent a real responsibility boundary such as:

```text
session/
session/state/
session/transport/
session/storage/
```

---

# 11. Go Profile

Pureline Go must remain idiomatic Go. Pureline does not try to make Go look like Java.

## PL-GO-001 — Error Guards

Preferred:

```go
user, err := loadUser(id)

if err != nil {
    return nil, err
}

session := createSession(user)
return session, nil
```

This naturally matches Pureline's guard-first philosophy.

## PL-GO-002 — Avoid `else` After Terminating Branches

Avoid:

```go
if err != nil {
    return nil, err
} else {
    return createSession(user), nil
}
```

Prefer:

```go
if err != nil {
    return nil, err
}

return createSession(user), nil
```

## PL-GO-003 — Naming Must Remain Idiomatic

Good:

```text
user
session
conn
server
packet
request
ctx
err
wg
tx
```

Avoid meaningless one-letter names unless they are strongly established local conventions.

## PL-GO-004 — Short Receiver Names Are Allowed

Preferred:

```go
func (s *Server) Start() error {
}
```

Receiver names are local, conventional, and unambiguous.

Pureline does not force:

```go
func (server *Server) Start() error {
}
```

when it adds no clarity.

## PL-GO-005 — Small Interfaces

Preferred:

```go
type Storage interface {
    Load(id ID) (*User, error)
    Save(user *User) error
}
```

Avoid oversized interfaces containing unrelated responsibilities.

## PL-GO-006 — Define Interfaces Near Consumers

Interfaces should generally be defined where they are consumed rather than globally pre-designed as large inheritance-style abstractions.

## PL-GO-007 — Use `defer` for Clear Resource Lifecycles

Preferred:

```go
file, err := os.Open(path)

if err != nil {
    return err
}

defer file.Close()
```

## PL-GO-008 — Goroutines Need Ownership

Avoid fire-and-forget goroutines without an explicit lifecycle.

A long-lived goroutine should have clear handling for cancellation, error propagation, shutdown, and ownership.

## PL-GO-009 — Avoid Premature Interface Abstraction

Do not create an interface merely because a concrete type might theoretically have another implementation later. Create abstractions when there is a real consumer need.

---

# 12. Crystal Profile

Crystal keeps its own language identity while following Pureline principles.

## PL-CR-001 — Idiomatic Postfix Guards Are Allowed

Crystal postfix conditions are Pureline-compliant when they express a short, obvious guard.

Good:

```crystal
return unless valid
return if closed
```

They fit Pureline well because they can keep guard logic compact and preserve a linear happy path.

Use an explicit block when either the condition or the guarded action becomes more complex:

```crystal
if session.closed? || session.expired?
  logger.info("Session is no longer usable")
  return
end
```

The rule is not "avoid postfix conditions." The rule is: use the form that keeps the intent immediately obvious.

## PL-CR-002 — Avoid `else`

Avoid:

```crystal
if valid
  execute
else
  reject
end
```

Prefer:

```crystal
if !valid
  reject
  return
end

execute
```

## PL-CR-003 — Idiomatic Naming

Variables and methods use `snake_case`:

```crystal
session_service
packet_registry
load_user
create_session
```

Types use PascalCase:

```crystal
SessionService
PacketRegistry
UserStorage
```

## PL-CR-004 — Prefer Type Inference

Preferred:

```crystal
session = create_session(user)
```

Avoid redundant annotations:

```crystal
session : Session = create_session(user)
```

when Crystal can infer the type clearly.

## PL-CR-005 — Explicit Nil Guards

Preferred:

```crystal
user = users.find(id)

if user.nil?
  return
end

session = create_session(user)
```

Avoid burying expected nil handling inside deeply nested chains.

## PL-CR-006 — Exceptions Are for Exceptional States

Expected outcomes should preferably use normal domain modeling such as `nil`, union types, result-like structures, or enums.

Exceptions should represent exceptional failure, not ordinary branching.

## PL-CR-007 — Avoid Macro Cleverness Without Need

Crystal macros are powerful, but Pureline avoids hiding ordinary behavior behind metaprogramming unless the abstraction produces a clear architectural benefit.

Generated behavior should remain predictable and discoverable.

---

## PL-CR-008 — Folder Density

A Crystal source or namespace folder may contain at most **10 production `.cr` files** by default.

Specs, generated files, bindings, and metadata-only files do not count.

When the limit is exceeded, split the namespace by responsibility while preserving idiomatic Crystal structure.

---

# 13. Emerald Profile

Pureline Emerald follows **native Emerald syntax** instead of borrowing the proposed Emvus syntax. Emerald is also the bootstrap language for Emvus, so this distinction matters for tools and compiler code.

## PL-EMS-001 — Keep Emerald Syntax Native

Emerald uses `.ems` files, `namespace` and `use` declarations with `::`, return types before method names, and semicolons.

```java
namespace App::Session;

use App::Model::Session;

public class SessionService {
    public Int square(Int value) {
        return value * value;
    }
}
```

Do **not** rewrite Emerald declarations as Emvus `package`, `method<T>`, `where`, or semicolon-free code. These are different languages.

## PL-EMS-002 — Use Semicolons Consistently

Follow Emerald's semicolon-based statement syntax. A formatter must not apply the Emvus optional-semicolon rule to `.ems` files.

```java
Int result = calculate(4);
println(result);
```

## PL-EMS-003 — Names and File Organization

- Types: `PascalCase`.
- Interfaces: `I` prefix (`IStorage`, `IPacket`).
- Methods, parameters, fields, and locals: `camelCase`.
- Constants: `UPPER_SNAKE_CASE` where applicable.
- Keep `namespace` and `use` declarations explicit.
- Prefer source files named after their primary type, respecting existing Emerald repository naming conventions (for example, `SessionService.ems`).
- Use at most **8** production `.ems` files per source/namespace folder by default; tests and generated sources are excluded.

Do not rename existing working source files merely to satisfy a hypothetical cross-language convention.

## PL-EMS-004 — Guard Clauses with Braces

Use explicit blocks and keep the successful path unnested.

```java
public Int normalized(Int value) {
    if (value < 0) {
        return 0;
    }

    return value;
}
```

Avoid `else` when an early `return`, `continue`, or other termination makes it unnecessary. Inline brace-free `if` statements are not Pureline Emerald.

## PL-EMS-005 — Explicit Types and `var` Are Both Valid

Use a declared type when it clarifies an API boundary. Use `var` where a statically inferred type is immediately obvious; `var` is **not** a request for dynamic typing.

```java
Person person = Person("Jason", 20);
var copy = person.copy();
```

Neither form should be flagged as a style violation merely because the other is possible.

## PL-EMS-006 — Preserve Emerald's Object Model

Use Emerald's normal constructor call form without a Java-style `new` keyword.

```java
Connection connection = Connection();
```

Keep constructors, fields, inheritance, and interfaces explicit. Prefer small cohesive classes over extra manager/factory layers with no concrete responsibility.

## PL-EMS-007 — Prefer Data Classes for Data-Shaped Types

When a type primarily bundles related data and Emerald's supported generated members are suitable, prefer a data class over repeated boilerplate.

```java
public data class Coordinate(Int x, Int y);
```

Emerald's data-class generation includes a constructor, `equals`, `toString`, `copy`, and `withX` methods where applicable. Do not assume Emvus-only data-class features such as named-argument `copy`, generated `hashCode`, or Emvus destructuring are already part of Emerald.

## PL-EMS-008 — Use Macros Only for Genuine Compile-Time Abstractions

Emerald's AST macro system is appropriate for repetitive structural generation, not for disguising ordinary program logic.

- Prefer small, clearly named annotation macros.
- Make generated fields and methods discoverable to maintainers and tooling.
- Avoid replacing a straightforward method with difficult-to-debug macro behavior.
- Do not rely on undocumented macro capabilities or silently mutate unrelated declarations.
- Keep compile-time transformations separate from runtime side effects.

## PL-EMS-009 — Make Error Paths Explicit

Use the error mechanism supported by the actual Emerald API. Where a function returns `Result<T, E>`, handle or forward it intentionally. Do not copy Emvus-specific `success value`, `fail error`, postfix `?`, or `scoped` constructs into Emerald code unless that syntax is separately implemented there.

Guard invalid input early and make unexpected failure visible rather than swallowing it.

## PL-EMS-010 — Concurrency Has Ownership

Emerald's `Fiber`, `@Async`, `@Synchronized`, and thread facilities should be used with explicit lifecycles.

- Keep blocking work off inappropriate execution threads.
- Await operations whose results or failures matter.
- Synchronize shared mutable state intentionally.
- Do not confuse asynchronous execution with automatic thread safety.

## PL-EMS-011 — Keep Flow-Sensitive Code Understandable

Emerald Flow may optimize allocation and object lifetimes, but readable source semantics take priority.

- Prefer stable local references when multiple reads would be ambiguous.
- Do not manually rely on undocumented `Folded`, `Inline`, `Local`, `Pool`, `Shard`, `Root`, `Bond`, or `Static` storage choices.
- Avoid hidden escapes and accidental long-lived references in hot paths.
- Treat optimizer strategies as compiler implementation details, not source-level coding rituals.

## PL-EMS-012 — Test Compiler-Facing Abstractions

For compiler, AST, and runtime code, write tests for boundary conditions, error paths, generated members, and object lifetime behavior. A shorter method is not automatically better if it hides a phase transition or important side effect.

---

# 14. Emvus Profile

Pureline Emvus follows the **current Emvus language design**. This is a style profile for intended `.ev` syntax, not a claim that every showcased feature is already implemented by Ring 1. Keep Emvus distinct from Emerald.

## PL-EV-001 — Native Method Declarations

Declare methods with `method` and, when needed, a return type in `method<Type>`. Generic type variables are introduced through `where`, not by attaching another generic parameter list to the method name.

```java
method<Int> square(Int value) {
    return value * value
}

method<T> first(List<T> items) where T {
    return items.get(0)
}
```

A method without an explicit return type returns `Void`.

## PL-EV-002 — Empty Declaration Parentheses Are Optional; Call Parentheses Are Not

Both parameterless declarations are Pureline-compliant:

```java
method refresh {
    updateCache()
}

method shutdown() {
    closeConnections()
}
```

Both spellings declare zero-argument methods; the difference is cosmetic, **not** an overload distinction. Always write `refresh()` or `shutdown()` to execute the method; `object.refresh` is not a method call. Formatters should preserve or consistently configure either valid declaration style without marking the other as an error.

## PL-EV-003 — Braces Required; Semicolons Optional

Use explicit `{ ... }` blocks for `if`, loops, and comparable control flow. Prefer omitting optional semicolons in idiomatic Emvus, but do not reject existing valid semicolon-terminated code.

```java
method process(Int value) {
    if (value < 0) {
        return
    }

    println(value)
}
```

Brace-free inline guards and unnecessary `else` blocks are not Pureline. Keep continuation lines and long expressions unambiguous.

## PL-EV-004 — Naming, Imports, and Visibility

- Types: `PascalCase`; interfaces: `I` prefix (`IStorage`, `IConnection`).
- Methods, fields, properties, parameters, and locals: `camelCase`.
- Constants: `UPPER_SNAKE_CASE`.
- Packages: lower-case dotted paths.
- Source filenames: **camelCase**, for example `sessionService.ev`, even if the primary class is `SessionService`.
- The default visibility is `public`; specify `private` or `protected` when required.
- Symbol resolution is case-insensitive but source spelling is preserved: use the **declared spelling** consistently and do not define names that differ only in case.

```java
package app.session

use {
    std.collections.[IList, IIterator],
    app.storage.userStorage as Storage
}
```

Use explicit imports and aliases instead of relying on confusing collisions.

## PL-EV-005 — Types Should Communicate Intent

Emvus is statically typed by default. `var` performs **static type inference**; `dynamic` explicitly requests runtime-typed behavior.

```java
Person person = Person("Jason", 20)
var current = person.copy()
dynamic payload = receiveUnknownPayload()
```

Use `dynamic` at genuinely dynamic boundaries, not to bypass normal type checks. Both explicit and inferred static local types are Pureline-compliant.

## PL-EV-006 — Handle Nullability Through Stable Flow

Emvus does not require a `T?` annotation. It tracks null states through control flow. Guard a possibly-null value early and prefer a stable local snapshot of mutable properties before reasoning about their nullability.

```java
method<String> displayName(Profile profile) {
    final var nickname = profile.nickname

    if (nickname == null) {
        return "Unknown"
    }

    return nickname
}
```

Never rely on repeated unsynchronized reads of a mutable property having the same value. Make required field initialization explicit.

## PL-EV-007 — Prefer Typed Result Handling for Expected Failures

Emvus has no `try/catch/finally` syntax. Use `Result<T, E>`, `success`, `fail`, `?`, and `switch` as the language specifies.

```java
method<Result<Int, RangeException>> validatePort(Int port) {
    if (port < 1 || port > 65535) {
        fail RangeException(port, 1, 65535)
    }

    success port
}

method showPort(Int port) {
    switch (validatePort(port)) {
        success(value) -> println(value)
        error(problem) -> errprintln(problem.message)
    }
}
```

- `?` is for deliberate error propagation where types permit it.
- `!` causes a panic on error; reserve it for established invariants or boundary checks that truly cannot recover.
- Do not discard a `Result` or hide expected failure behind `panic`.
- Keep `success`, `fail`, and `return` distinct rather than wrapping every operation unnecessarily.

## PL-EV-008 — Make Resource Lifetimes Visible with `scoped`

Use Emvus's `scoped` construct for resources that must be closed when their scope ends.

```java
method<String> readFile(String path) {
    scoped var file = Files.open(path)
    return file.readAll()
}
```

Do not replace `scoped` with Java `try-with-resources`, Go `defer`, or a nonexistent Emvus `finally`. A resource's lifetime should remain clear even when the compiler optimizes memory.

## PL-EV-009 — Prefer Data Classes to Boilerplate

Use `data class` for small, named, data-centric types with the built-in generated behavior.

```java
data class Member(String name, Int age)

data class Server(String host, Int port = 25565)
```

Primary data properties are read-only outside initialization. `copy()` and `withX(...)` express modified copies and are shallow by default. Prefer generated equality, hashing, and textual representation where appropriate rather than rewriting them manually. Keep larger behavior-rich types as regular classes.

**Do not introduce a separate `record` style requirement:** the agreed Emvus design uses `data class` for named data models and `Dictionary` for dictionary/object-literal data.

## PL-EV-010 — Lists, Tuples, and Dictionaries Retain Their Defined Semantics

For `[...]` literals, **all element types exactly equal** implies a `List`; otherwise the expression is a tuple. Do not make Pureline overwrite the language's type-driven rule with a stylistic preference.

```java
var numbers = [10, 20, 30]        // List<Int>
var response = [200, "Success"]   // [Int, String]
```

Dictionary literals use `{ key: value }`; inferred mixed value types use `dynamic` where required by the language model. Dictionaries are accessed by **keys**, not structural-record dot notation.

```java
var server = {
    "name": "Emvus",
    "port": 25565
}

println(server["name"])
```

A changing element type can legitimately change list-versus-tuple inference. Keep that behavior visible at API boundaries through explicit declared types where appropriate.

## PL-EV-011 — Destructuring Is Positional

Use `{ ... }` to destructure tuple or data-class components in declaration order. `_` may skip an element.

```java
var { status, message } = [200, "OK"]
var { first, _ } = loadPair()
```

Do not reinterpret destructuring as named record matching. Prefer short meaningful bindings, not indiscriminate unpacking.

## PL-EV-012 — `switch` Should Expose Complete Branching

Use `switch` statements or expressions for real state alternatives. Expressions must be exhaustive or have a defined catch-all / failure arm.

```java
method<String> rankLabel(Rank rank) {
    return switch (rank) {
        MEMBER -> "Member"
        MODERATOR -> "Moderator"
        ADMIN -> "Admin"
    }
}
```

Prefer clear `success(...)` and `error(...)` patterns to nested conditions for `Result`. Keep catch-all patterns deliberate, not a way to hide forgotten cases.

## PL-EV-013 — Use Cascades Only When They Clarify a Single Receiver

Cascades with `..` are useful for a short sequence of operations on the same receiver.

```java
Person person = Person("Anna", 20)
    ..birthday()
    ..age = 21
```

Avoid chains that conceal unrelated effects, mutate several objects implicitly, or stretch across many statements.

## PL-EV-014 — Fibers and Threads Need Clear Lifecycles

`fiber {}`, `virtual {}`, and `thread {}` are distinct execution constructs with a common task-result pattern. Choose based on the actual workload, not because concurrency looks compact.

```java
var task = fiber {
    return calculate(10)
}

Int result = task.await()
```

- Await results and failures that matter.
- Make cancellation and ownership clear for long-lived tasks.
- Synchronize shared mutable state; `@Synchronized` is available where appropriate.
- Do not assume a task's captured object is stack-local or can be released when the spawning method returns.
- Avoid hidden fire-and-forget work in ordinary business methods.

## PL-EV-015 — Macros Must Improve Discoverability, Not Obscure It

Emvus macros are compile-time AST transformations with target declarations, `quote`, `$(...)` substitution, and AST builder APIs.

```java
macro Trace on Method {
    target.body.prepend(quote statement {
        println("Method entered")
    })
}
```

Pureline requires:

- Small macros with clearly stated responsibilities and supported targets.
- Deterministic expansion; source-order behavior and resulting members remain discoverable.
- Hygiene to avoid accidental name collisions.
- Compiler diagnostics when expansion creates conflicting or invalid code.
- Ordinary methods instead of macros when no substantial compile-time benefit exists.
- No reliance on unreviewed macro-time I/O, network, clock, or random side effects.

Generated code must satisfy semantic checks; checker/formatter tooling should not treat generated locations as ordinary hand-maintained files.

## PL-EV-016 — Do Not Program Against HyperAlloc Flow Internals

Emvus code may be optimized through Sublang, Optimind, and HyperAlloc Flow. Developers should express **semantics and lifetimes**, not manually guess a future allocation strategy.

- Favor straightforward object lifecycles and stable ownership.
- Keep shared mutable state explicit, especially across tasks.
- Prefer immutable data classes for simple values when they fit the domain.
- Do not build APIs around assumptions that an object is always stack-allocated, scalar-replaced, or pooled.
- Evaluate hot-path allocation and escape behavior through measurement and diagnostics, not folklore.

## PL-EV-017 — Package Density and Source Organization

A package/source folder should contain at most **8** production `.ev` modules by default. Tests, generated sources, and metadata-only files do not count. Split by actual feature responsibility rather than mechanically creating arbitrary subpackages.

A package name and a filename are different concepts: `app.session` remains lower-case, while `sessionService.ev` is `camelCase`.

## PL-EV-018 — Language-Native Rules Override Cross-Language Imitation

Pureline tooling must not report a violation merely because Emvus differs from Java, Emerald, Rust, or Go:

- Optional `()` in **parameterless method declarations** are valid; calls always require `()`.
- Optional semicolons are valid.
- `where` is the native generic declaration syntax.
- `var` is statically inferred; `dynamic` is explicitly dynamic.
- `success`, `fail`, `?`, `!`, `scoped`, and AST macros have Emvus-specific meanings.
- `{ ... }` has valid context-dependent uses, including dictionaries and positional destructuring.
- `[ ... ]` is typed as a list or tuple according to Emvus's exact-element-type rule.

The formatter should be AST-aware, preserve meaning, and never apply Emerald's `::` namespaces or semicolon requirements to `.ev` files.

---

# 15. Rust Profile

Pureline Rust follows **idiomatic Rust** rather than translating Java conventions into Rust syntax. Ownership, lifetimes, enums, pattern matching, and the type system are core strengths, not obstacles to work around.

## PL-RS-001 — Idiomatic Naming and Visibility

- Types, traits, and enum variants: `PascalCase`.
- Functions, methods, modules, fields, and locals: `snake_case`.
- Constants and statics: `SCREAMING_SNAKE_CASE`.
- Crates: descriptive `snake_case` names (Cargo package names may use hyphens).
- Traits do **not** inherit the Java `I` prefix. Prefer `Storage`, not `IStorage`.
- Keep items private unless an actual consumer needs wider visibility; prefer `pub(crate)` when suitable.
- Use `use` imports rather than repeating long fully-qualified paths, except where explicit qualification resolves ambiguity.

## PL-RS-002 — Guard Clauses and Linear Flow

Validate early and keep the successful path at the base indentation level.

```rust
fn open_session(request: &Request) -> Result<Session, SessionError> {
    if !request.is_valid() {
        return Err(SessionError::InvalidRequest);
    }

    let user = request.user().ok_or(SessionError::MissingUser)?;
    Session::open(user)
}
```

Avoid `else` after a branch that already returns, breaks, or continues. An `if`/`else` **expression** is allowed for genuine two-way value selection where it is clearer than alternatives. `let ... else` is a **preferred Rust guard construct**, not a violation of the avoid-`else` principle.

```rust
let Some(session) = sessions.get(&id) else {
    return Err(SessionError::NotFound);
};

session.start()
```

## PL-RS-003 — Prefer `Result`, `Option`, and `?`

- Represent expected failures with `Result<T, E>`.
- Represent absence with `Option<T>`.
- Use `?` for straightforward propagation and `map_err`/`ok_or_else` when error context genuinely improves.
- Use `match` when outcomes need distinct handling; avoid convoluted chains of combinators.
- Do not discard `Result` values silently; decide whether to propagate, log, recover, or deliberately ignore with a justified explanation.

```rust
fn load_config(path: &Path) -> Result<Config, ConfigError> {
    let content = std::fs::read_to_string(path)?;
    let config = toml::from_str(&content)?;
    Ok(config)
}
```

## PL-RS-004 — Restrict `unwrap`, `expect`, and `panic!`

Never use `unwrap()` or `expect()` for ordinary recoverable production failures. They are acceptable in tests, prototypes, or documented invariants that really cannot be violated. `panic!` is not a normal error-handling strategy. Prefer explicit handling over hidden failure via unchecked indexing or conversions.

## PL-RS-005 — Use `match` and Patterns for Real State Branching

Use exhaustive `match` for enums and meaningful alternatives; use `if let` or `let ... else` for simple single-pattern handling. Do not force `match` onto ordinary boolean checks. Avoid broad `_` arms that silently mask future states when enumerating variants would be clearer.

```rust
fn label(state: SessionState) -> &'static str {
    match state {
        SessionState::Ready => "Ready",
        SessionState::Closed => "Closed",
        SessionState::Failed => "Failed",
    }
}
```

## PL-RS-006 — Ownership Must Be Deliberate

- Prefer borrowing `&T` or `&mut T` when ownership transfer is unnecessary.
- Accept owned values when the callee genuinely needs ownership.
- Minimize cloning; use `.clone()` when its ownership cost and purpose are clear.
- Avoid needless `Arc`, `Rc`, `Box`, and interior mutability wrappers.
- Prefer short lifetimes and explicit ownership boundaries; do not fight the borrow checker with artificial leaks or pervasive shared ownership.

## PL-RS-007 — Immutable by Default

Use `let` unless mutation is needed. Keep `let mut` local and narrowly scoped. Avoid interior mutability as a default substitute for ordinary ownership design. Do not create unnecessary mutable globals.

## PL-RS-008 — Traits Require a Real Abstraction

Create a trait for an actual shared behavior, API boundary, interchangeable implementation, or test seam. Do not introduce traits merely to mirror Java interfaces or create factory/manager layers. Prefer simple inherent `impl` methods for single concrete implementations. Choose generics or trait objects based on actual polymorphism requirements.

## PL-RS-009 — Cohesion over Size for `struct`, `impl`, and Modules

`PL-STRUCT-003` applies to Rust as a **cohesion review**, not as a fixed line-count limit. A cohesive `impl` or state machine may be long. Split by independent responsibility, not by mechanically extracting traits, structs, or files. Avoid huge unrelated `mod.rs` or `lib.rs` dumping grounds.

## PL-RS-010 — Module and File Density

A Rust feature/source folder should contain at most **10 hand-maintained production `.rs` files** by default. Tests, generated code, bindings, and metadata-only files do not count. `mod.rs`, `lib.rs`, and `main.rs` count when they contain substantial maintained logic, not when serving only as module declarations/re-exports. This is a review threshold: split along actual responsibility boundaries, never into meaningless nested folders.

Prefer explicit modules with narrow public surfaces; keep internal implementation private.

## PL-RS-011 — Use Rustfmt as the Formatting Authority

- Follow `rustfmt` / `cargo fmt` for indentation, line wrapping, trailing commas, and idiomatic formatting.
- Prefer compact expressions and calls **when rustfmt supports them**; Pureline must not demand manual formatting that rustfmt will undo.
- Write clear expression structure rather than using formatting directives to fight the formatter.
- Use ordinary brace-delimited control flow; do not emulate Java semicolon rules inside expression-oriented Rust.

This rule overrides language-independent horizontal-call preferences when they conflict with the canonical Rust formatter.

## PL-RS-012 — Prefer Clear Iteration over Clever Chains

Iterators, closures, and combinators are encouraged when they directly express a transformation. Prefer a regular loop when it makes branching, side effects, borrowing, debugging, or hot-path behavior clearer. Avoid chaining adapters simply to reduce line count.

## PL-RS-013 — Async Tasks Have Ownership and Shutdown

- Use `async`/`.await` for asynchronous workloads where appropriate.
- Do not block an async executor with long-running synchronous I/O or CPU work; use dedicated blocking execution or worker threads as needed.
- Track spawned tasks, cancellation, error propagation, and orderly shutdown.
- Do not create hidden fire-and-forget tasks for important work.
- Use channels and synchronization primitives with clearly defined ownership and backpressure.
- Never hold a blocking or asynchronous lock across `.await` unless the design explicitly requires and safely supports it.

## PL-RS-014 — Resource Lifecycles Should Be Predictable

Prefer RAII and `Drop` for synchronous cleanup. Explicitly await or coordinate shutdown for asynchronous resources because `Drop` cannot itself await. Expose a `close()` / `shutdown()` method when the operation can fail or must complete before process exit. Avoid hiding significant fallible cleanup inside destructors alone.

## PL-RS-015 — Unsafe Code Must Be Isolated and Justified

Safe Rust is the default. Use `unsafe` only where required by a genuine FFI, hardware, or performance boundary. Keep unsafe regions small, document their invariants with `// SAFETY:`, encapsulate them behind a minimal safe API, and test the boundary. Do not use `unsafe` simply to bypass normal ownership checks.

## PL-RS-016 — FFI and Platform Boundaries Are Explicit

For Wayland, GPU, system libraries, and similar native integration, keep raw handles, lifetime guarantees, ownership transfer, threading assumptions, and error conversion visible. Isolate platform-specific code in focused modules. Generated bindings may be excluded from ordinary formatting and density limits.

## PL-RS-017 — Prefer Types that Encode Invariants

Use enums and small newtypes where they prevent real invalid states or clarify external boundaries. Do not create wrappers for every primitive without benefit. Prefer constructors that validate invariants and expose clear errors. Make invalid or uninitialized states difficult to represent.

## PL-RS-018 — Avoid Macro Cleverness

Prefer functions, traits, enums, and straightforward generics over macros for routine behavior. Use declarative or procedural macros when they eliminate meaningful structural repetition. Keep generated behavior discoverable and diagnostics understandable.

## PL-RS-019 — Measure Performance Before Complexity

Avoid unnecessary allocations, clones, reference counting, and locking in measured hot paths. Prefer predictable ownership and data structures suited to the workload. Profile before replacing clean code with complex low-level optimizations, and validate latency as well as throughput for UI/compositor workloads.

## PL-RS-020 — Testing and Tooling

- Run `cargo fmt --check` for formatting and `cargo clippy` with project-agreed lint settings.
- Test normal, invalid, and failure paths, especially cancellation, lifecycle, FFI, and concurrency boundaries.
- Make warnings intentional; avoid blanket `allow` attributes to silence broad lint groups.
- Pureline Checker must use Rust syntax/AST-aware inspection (including `let ... else`, pattern matching, macros, and expression returns), not textual pattern matching.

---

# 16. Abstraction Rules

## PL-ARCH-001 — Abstraction Requires a Reason

Avoid abstractions such as:

```text
AbstractBaseFactoryManagerProvider
```

merely to reduce duplication.

Good reasons for abstraction include:

```text
multiple real implementations
clear API boundaries
platform-specific implementations
test seams
dependency inversion
shared domain contracts
```

---

# 17. Architecture

Pureline prefers clear dependency direction.

Example:

```text
domain
↓
service
↓
infrastructure
```

or feature-oriented modules:

```text
session/
network/
storage/
command/
runtime/
api/
```

Avoid cycles such as:

```text
network → session → storage → network
```

---

# 18. Side Effects

Important side effects should remain visible.

Preferred:

```java
var user = users.load(id);
var session = sessions.create(user);

sessions.save(session);
events.publish(new SessionCreated(session.id()));
```

Potentially misleading:

```java
sessions.createAndPersistAndNotify(user);
```

if persistence and event publication are important externally observable actions.

Pureline does not require exposing every internal operation. It requires avoiding surprising hidden behavior.

---

# 19. Immutability

Prefer immutable values by default.

Mutation should be intentional, local, and easy to identify.

Preferred examples include Java records, TypeScript readonly data, `const` in JavaScript, immutable value objects, and short-lived mutable local state.

Avoid shared mutable state where practical.

---

# 20. Dependencies

Dependencies should be visible in constructors, function parameters, or explicit module wiring.

Avoid hidden global access patterns and unnecessary service locators.

Dependency injection is encouraged where it improves architecture, but Pureline does not require a heavy DI framework.

---

# 21. Performance

Pureline prioritizes readability by default, but performance-sensitive paths should be written deliberately.

In hot paths:

- avoid unnecessary allocations
- avoid repeated reflection
- cache expensive resolution
- use appropriate collections
- avoid hidden blocking
- reduce unnecessary copying
- keep ownership clear

Performance optimizations must remain understandable.

---

# 22. Language Profiles

```text
Pureline Core
│
├── Pureline Java
│   ├── Records
│   ├── I-prefixed interfaces
│   ├── Modern Java
│   ├── Explicit null handling
│   └── Virtual Threads
│
├── Pureline JavaScript
│   ├── const-first
│   ├── semicolons
│   ├── braces
│   └── async/await
│
├── Pureline TypeScript
│   ├── strong typing
│   ├── inference
│   ├── unknown > any
│   └── unions/types
│
├── Pureline Go
│   ├── idiomatic error guards
│   ├── small interfaces
│   ├── defer
│   └── explicit goroutine lifecycle
│
├── Pureline Crystal
│   ├── idiomatic postfix guards
│   ├── explicit blocks for complex flow
│   ├── nil guards
│   └── type inference
│
├── Pureline Rust
│   ├── Result / Option / ? and let-else guards
│   ├── ownership, borrowing, RAII
│   ├── exhaustive match and enums
│   ├── rustfmt / clippy
│   └── explicit async task lifecycles
│
├── Pureline Emerald (.ems)
│   ├── namespace / use with ::
│   ├── typed declarations and semicolons
│   ├── data classes and AST macros
│   └── Flow-aware lifecycle clarity
│
└── Pureline Emvus (.ev)
    ├── method<Type> and where generics
    ├── Result, fail/success, scoped
    ├── data classes, dictionaries, tuples
    ├── AST macros
    └── fiber / virtual / thread
```

---

# 23. Tooling Model

Pureline tooling should share the same rule specification across:

```text
Pureline Checker
Pureline Formatter
Pureline CLI
IntelliJ Plugin
VS Code Extension
Eclipse Plugin
future IDE integrations
```

Diagnostics should reference stable rule IDs.

Example:

```text
PL-CF-002
Avoid else after a terminating branch.

Line 48:
} else {

Suggested fix:
Remove the else block and continue with the happy path.
```

This allows the website, CLI, formatter, linter, and IDE integrations to stay consistent.

**Emerald and Emvus tooling requirement:** use separate grammars for `.ems` and `.ev`. The checker must distinguish real syntax and resolved AST constructs from textual lookalikes; macro-generated declarations, inferred tuple/list forms, optional method-declaration parentheses, and optional Emvus semicolons require language-aware handling. Existing and newly added profile IDs must produce consistent diagnostics across tools.

**Versioning:** Rust rules use stable `PL-RS-*` IDs; Emerald and Emvus rules retain `PL-EMS-*` and `PL-EV-*` IDs. The established `PL-CORE-*`, `PL-CF-*`, `PL-JAVA-*`, `PL-JS-*`, `PL-TS-*`, `PL-GO-*`, and `PL-CR-*` IDs are not renumbered. Version 1.4 added `PL-CORE-022` and `PL-STRUCT-003`; version 1.5 adds `PL-RS-001`–`PL-RS-020` without renumbering existing rules.

---

# 24. Pureline Summary

Pureline code should feel:

- linear
- explicit
- compact
- modern
- low-noise
- modular
- cohesive regardless of line count
- predictable
- easy to scan
- easy to refactor
- hard to misunderstand

The core rule remains:

> **Code without noise.**

A second principle governs every language profile:

> **Pureline standardizes principles, not syntax.**

A language should remain idiomatic. Pureline removes unnecessary noise without stripping away the features that make Java, JavaScript, TypeScript, Go, Crystal, Rust, Emerald, or Emvus effective in the first place.
