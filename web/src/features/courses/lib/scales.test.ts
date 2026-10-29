import { readChosen, shownScales } from "./scales";

const all = [
  { id: "pct", builtIn: true },
  { id: "nl", builtIn: true },
  { id: "de", builtIn: true },
  { id: "mine", builtIn: false },
];

describe("shownScales", () => {
  it("shows everything until you choose", () => {
    expect(shownScales(all, null).map((s) => s.id)).toEqual(["pct", "nl", "de", "mine"]);
  });
  it("keeps your own scales and the course's current one", () => {
    expect(shownScales(all, ["nl"]).map((s) => s.id)).toEqual(["nl", "mine"]);
    expect(shownScales(all, ["nl"], "de").map((s) => s.id)).toEqual(["nl", "de", "mine"]);
  });
  it("reads the stored choice", () => {
    expect(readChosen({ gradeScales: ["a"] })).toEqual(["a"]);
    expect(readChosen({ gradeScales: "a" })).toBeNull();
    expect(readChosen({})).toBeNull();
    expect(readChosen(null)).toBeNull();
  });
});
