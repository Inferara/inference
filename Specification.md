# Inference specification

### Explanation of the Inference Language Keywords and Constructs

#### 1. **Context Declaration**

```inference
context Polkadot;
```

- `context` : Specifies the environment or a platform for which the specification is written. Here, the context is set to "Polkadot", indicating that the specifications are relevant to the Polkadot blockchain.
- `Polkadot` : The name of the context or platform for which the specifications are written.
- `;` : The semicolon is used to terminate the statement.

#### 2. **Context Level Parameters**

```inference
type : blockchain; ///smart_contract
finality : GRANDPA;
consensus : POS;
```

- `type` : Indicates the type of the system being modeled. In this example, it uses the `blockchain` from the `inference std`.
- `///` : Indicates a comment in the code. Multiline commens are not supported.
- `finality` : Specifies the finality mechanism used in the blockchain, here it is the type `GRANDPA` from the `inference std`.
- `consensus` : Specifies the consensus mechanism, here it is `POS` (Proof of Stake) from the `inference std`.

#### 3. **Enumeration Declaration**

```inference
enum Preservation { Expendable, Preserve, Protect }
```

- `enum` : Defines an enumeration, which is a distinct data type consisting of a set of named values called elements or members. Here, `Preservation` is an enumeration with three possible values: `Expendable`, `Preserve`, and `Protect`.

#### 4. **Type Definition**

```inference
type address : bytes[140] {
  balance : nat;
  is_active : bool;
}
```

- `type`: Defines a new type named `address` as a byte array of length 140.
- `balance` : A field within the `address` type, specified as a natural number (type `nat`).
- `is_active`: A field within the `address` type, specified as a `boolean`.

#### 5. **Actor Definition**

```inference
actor user : address {
  can_emit : [ transfer, transfer_to_zero_address ];
}

actor smart_contract : address {
  can_emit : [ transfer, transfer_to_zero_address ];
}
```

- `actor` : Defines an actor in the system, which is a type that can participate in various processes.
- `can_emit` : Specifies the list of signals that the actor can emit.

Here, both `user` and `smart_contract` actors can emit `transfer` and `transfer_to_zero_address` signals.

#### 6. **Signal Definition**

```inference
signal transfer_to_zero_address {
  forall(src: address, value: nat),
  transfer_allow_death(src, 0, value, Preservation::Expendable) -> Result[(), exit(1)];
}

signal transfer {
  forall(src : address, dst : address, value : nat),
  transfer_allow_death(src, dst, value, Preservation::Expendable) -> Result[(), exit(1)];
}

signal _transfer {
  forall(src : address, dst : address, value : nat, preservation : Preservation),
  transfer(src, dst, value, preservation) => Result[(), exit(1)];
}

signal call_ensure_signed {
  forall (a : address), ensure_signed(stack a) -> true;
}

signal call_lookup {
  forall (a : address), lookup(stack a) -> address;
}
```

- `signal` : Defines an event or action that can occur within the system.
- `forall` : Specifies that the signal is applicable for all instances of the given variable type.
- `transfer_to_zero_address`, `transfer`, `_transfer`, `call_ensure_signed`, `call_lookup` : Signals names.
- `transfer_allow_death`, `ensure_signed`, `lookup` : Functions or actions that the signal triggers, returning a result or causing the system to exit with a code.

#### 7. **Function Definition**

```inference
fn transfer_allow_death(transfer_to_zero_address, transfer) -> Result[(), exit(1)] {
  let source = ensure_signed($.src)?;
  let dest = lookup($.src)?;
  transfer(source, dest, value, Preservation);
}

fn transfer(_transfer) -> Result[(), exit(1)] {
  if $.source.balance lesseq $.value then return exit(1);
  $.source.balance = $.source.balance - $.value;
  $.dest.balance = $.dest.balance + $.value;
  match preservation with
  | Preservation::Expendable => {
      $.source.balance gt 0 then () else {
        $.source.is_active = false;
        return ();
      }
  }
  | _ => ();
}

fn ensure_signed(call_ensure_signed) -> bool => true;
fn lookup(call_lookup) -> address => $.address;
```

- `fn` : Defines a function.
- `transfer_allow_death`, `transfer`, `ensure_signed`, `lookup`: Functions with specified behaviors.
- `let` : Introduces a new variable with automatic type deduction.
- `if ... then ... else ...` : Conditional statement.
- `match ... with ...` : Pattern matching statement.

#### 8. **Process Definition**

```inference
process transfer_allow_kill_account {
  { user, smart_contract } emit { transfer, transfer_to_zero_address };
}

```
- `process` : Defines a process in the system.
- `emit` : Specifies the signals that the actors can emit within this process.

#### 9. **Axiom and Theorem Definition**

```inference
axiom G(address is actor) U address.balance lesseq 0;
theorem G(user.is_active) U emit transfer(user.address, _, user.balance, Preservation::Expendable);
```

- `axiom` : Defines an invariant or a truth that holds universally within the system.
- `theorem` : Defines a property that should be proven within the system.
- `G`: Globally (always) in temporal logic.
- `U` : Until in temporal logic.
