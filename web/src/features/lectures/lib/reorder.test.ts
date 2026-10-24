import { reorder } from "../api";

const list = [
  { id: "a", sortOrder: 10 },
  { id: "b", sortOrder: 20 },
  { id: "c", sortOrder: 30 },
];

it("patches only what moved", () => {
  expect(reorder(list, 2, 0)).toEqual([
    { id: "c", sortOrder: 10 },
    { id: "a", sortOrder: 20 },
    { id: "b", sortOrder: 30 },
  ]);
  expect(reorder(list, 0, 1)).toEqual([
    { id: "b", sortOrder: 10 },
    { id: "a", sortOrder: 20 },
  ]);
});

it("ignores no-ops and out-of-range moves", () => {
  expect(reorder(list, 1, 1)).toEqual([]);
  expect(reorder(list, -1, 2)).toEqual([]);
  expect(reorder(list, 0, 5)).toEqual([]);
});

it("renumbers uneven orders", () => {
  expect(
    reorder(
      [
        { id: "x", sortOrder: 1 },
        { id: "y", sortOrder: 7 },
      ],
      1,
      0,
    ),
  ).toEqual([
    { id: "y", sortOrder: 10 },
    { id: "x", sortOrder: 20 },
  ]);
});
