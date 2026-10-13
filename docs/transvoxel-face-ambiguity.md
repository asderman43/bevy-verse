# Face ambiguity: our triangle table vs. Transvoxel

**Question:** to add Transvoxel LOD transitions, do we have to replace our
`TRIANGLE_TABLE` with Lengyel's regular-cell table?

**Answer: no.** Both tables resolve every ambiguous face with the same rule.
Lengyel's transition tables can be fed our case index unchanged, as long as
their triangle winding is reversed.

Checked 2026-10-01 against `src/voxel_terrain/isosurface/tables.rs` and
Lengyel's reference tables (`Transvoxel.cpp`, from
<https://github.com/EricLengyel/Transvoxel>).

## Why it matters

A cube face is **ambiguous** when its two set corners lie on a diagonal. The
surface crosses all four face edges as two segments, and those can pair up in
two ways:

```
 S ─── E        S ─── E          S ─── E
 │     │        │ \   │          │   / │
 │  ?  │   →    │\  \ │    or    │  /  │      (S = set, E = clear)
 │     │        │ \   │          │ /  /│
 E ─── S        E ─── S          E ─── S
               set corners       set corners
               separated         joined
```

Both are valid, but the two cells that share a face must pick the same one, or
a hole opens between them. The original 1987 table broke this by treating a
case and its inverse as the same shape.

Transvoxel adds **transition cells** between chunks of different LOD. Their
faces meet regular cells on both sides:

- the coarse side meets the shrunk regular cells of the same chunk,
- the 3×3-sample fine side meets four regular cells of the finer neighbour.

So the transition tables only close the seam if our regular table resolves
ambiguous faces the way Lengyel's regular table does.

## Method

Both tables number corners the same way (`x` = bit 0, `y` = bit 1, `z` = bit
2), so they can be compared directly.

1. For every case (0–255) and each of the 6 faces, keep the faces where the
   two set corners are diagonal.
2. Collect the triangle edges that lie in that face: the edges whose two
   vertices are both on edges of the face. Keep those seen an odd number of
   times, since an edge shared by two in-face triangles is interior.
3. Each resulting segment joins two adjacent face edges and so cuts off the
   corner they share. Record whether the cut-off corners are the set ones
   (set corners separated) or the clear ones (set corners joined).
4. **Hole check:** group the results by (axis, face-local corner pattern),
   merging a cell's high face with the next cell's low face. Every group must
   contain one answer only.
5. **Winding:** on every case with a single set or single clear corner,
   compute each triangle's right-hand-rule normal and check whether it points
   toward the set side.

## Results

| | bevy-verse `TRIANGLE_TABLE` | Lengyel regular cells |
|---|---|---|
| Ambiguous (case, face) pairs | 192 | 192 |
| Resolution | set corners separated in **all 192** | set corners separated in **all 192** |
| Hole-free between neighbours | yes | yes |
| Normals point toward | set corners (16/16) | clear corners (16/16) |
| A set bit means | corner `<= isolevel`, **empty** | sample's sign bit: negative, **solid**\* |
| Physically | empty corners separated, solid joins diagonally | solid corners separated |

\* Inferred from his winding, assuming counter-clockwise front faces as in
Bevy. The compatibility result below does not depend on this assumption.

Both tables use one fixed, sign-only rule: **keep set corners apart**. They
differ only in which physical side the set bit stands for.

## Consequences for Transvoxel

- **Keep our `TRIANGLE_TABLE`.** None of the cross-validation tests built
  around it have to change.
- **Build the transition case index like ours:** a set bit means empty. Then
  the transition tables keep empty corners apart on every face, the same as
  our regular cells.
- **Reverse the winding of every transition triangle**, on top of the
  inversion bit that `transitionCellClass` already carries (bit 7, which
  means reverse). This follows only from the measured winding: with the same
  bit meaning, his normals point to the opposite side of ours.

## Still unverified

This compares the **regular** tables only. Transvoxel is only watertight if
the transition cells' own faces follow the same rule:

- the 4-corner coarse face,
- each 2×2 sub-face of the 9-sample fine face.

They should, since Lengyel built them to fit his regular table. Extending the
check to `transitionCellClass`, `transitionCellData` and
`transitionVertexData` would confirm it before Transvoxel is built.

A seam-watertightness test is still needed when Transvoxel is built: mesh a
fine chunk next to a coarse chunk with its transition mesh and require no open
edge along the seam.
