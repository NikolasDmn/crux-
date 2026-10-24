import { canEdit, isOwner, normalizeCode } from "./roles";

it("knows who may edit", () => {
  expect([canEdit("Owner"), canEdit("Editor"), canEdit("Viewer"), canEdit(undefined)]).toEqual([
    true,
    true,
    false,
    false,
  ]);
  expect([isOwner("Owner"), isOwner("Editor")]).toEqual([true, false]);
});

it("normalises course codes", () => {
  expect(normalizeCode("bsa-101 x")).toBe("BSA101X");
  expect(normalizeCode("abcdefghijklmnop")).toBe("ABCDEFGHIJKL");
});
