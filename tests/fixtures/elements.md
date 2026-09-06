# Heading 1

Every construct in the README element table lives in this file. The snapshot
tests render it at a fixed width, so anything the renderer changes shows up as
a diff here first.

## Heading 2

### Heading 3

#### Heading 4

##### Heading 5

###### Heading 6

A paragraph with *emphasis*, **strong**, ~~strikethrough~~, and `inline code`
in it, long enough that it has to wrap at least once at eighty columns.

A paragraph with a soft
break in it, and one with a hard\
break in it.

## Code

```rust
fn main() {
    println!("syntect highlighting arrives in milestone 3");
}
```

```
An unlabelled fence.
A line long enough that it has to be truncated rather than wrapped, because code blocks never wrap.
```

```notalanguage
A fence tagged with a language nothing knows.
```

```
#!/usr/bin/env python3
print("a shebang names the language when the fence tag does not")
```

## Quotes

> A block quote, long enough to wrap inside its own gutter so the continuation
> lines line up.
>
> > A nested quote, with **strong** and a [link](https://example.com) inside it.

## Lists

- A bullet item
- Another one, long enough that it wraps and its continuation lines hang under
  the text rather than the bullet
  - A nested item
    - A doubly nested item

3. An ordered list that starts at three
4. And counts on from there

- [x] A finished task
- [ ] An unfinished task

## Tables

| Left | Center | Right |
| :--- | :----: | ----: |
| a | b | c |
| one | two | three |

| Construct | What it is for |
| --- | --- |
| A table wider than the wrap width | so that the columns have to shrink proportionally and the cells inside them have to wrap onto more than one line |

## Links

An [external link](https://example.com/docs), a [local link](other.md), a
[local link with a fragment](nested/note.md#a-heading), a [fragment on this
page](#lists), a wikilink [[note]], an aliased one [[note|with an alias]], and
one with a fragment [[note#A Heading]].

<https://example.com/autolink> and <otavio@example.com>.

## Images

![a cat, sitting](cat.png)

## Footnotes

A sentence with a footnote reference[^1].

[^1]: The definition, long enough that it wraps and hangs under its marker.

## Raw HTML

<div class="note">
  <p>A block of raw HTML, shown verbatim.</p>
</div>

A paragraph with <em>inline HTML</em> in it.

## Wide characters

日本語のテキストは一文字が二桁分の幅を取るので、折り返しの計算がちがいます。

## Rules

---
