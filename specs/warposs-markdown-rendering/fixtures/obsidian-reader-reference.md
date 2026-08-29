# Markdown reading reference

This fixture validates the shared Warposs reading surface against the documented Obsidian
Reading view model. 正文需要在可读宽度内居中，保持舒适的行距，同时允许选中并复制文本。

## Information hierarchy

### Third-level heading

#### Fourth-level heading

##### Fifth-level heading

###### Sixth-level heading

Body text supports **bold**, *italic*, ~~strikethrough~~, `inline code`, and a
[trusted documentation link](https://obsidian.md/help/edit-and-read).

> A quoted paragraph keeps a visible rail and readable inline formatting.
>
> - Nested quote list item
> - 第二个引用列表项
> > A nested quote retains a second visual rail.

---

- Unordered item
  - Nested unordered item
1. Ordered item
2. 第二个有序项
- [ ] Open task
- [x] Completed task

```rust
fn reading_surface(source: &str) -> Result<Document> {
    Document::parse(source)
}
```

| Surface | Provenance | Reading | Source | Selection | Overflow behavior |
| --- | --- | --- | --- | --- | --- |
| Markdown file | Authoritative bytes | Shared rich reader | Exact source | Rendered text and source | Wide tables scroll horizontally |
| Agent CLI tab | Retained terminal grid | Shared rich reader | Unavailable | Rendered text only | Wide tables scroll horizontally |

The final paragraph verifies bottom spacing, selection across inline styles, and stable wrapping
after resizing the pane from a wide layout to a narrow layout and back again.
