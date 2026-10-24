import { flashKey, mockKey, typingIn, type KeyLike } from "./keyboard";

const k = (code: string, mods: Partial<KeyLike> = {}): KeyLike => ({
  code,
  key: "",
  shiftKey: false,
  altKey: false,
  ctrlKey: false,
  metaKey: false,
  ...mods,
});

describe("mock keys", () => {
  it("maps digits to options with confidence by modifier (event.code, so Shift+1 is still 1)", () => {
    expect(mockKey(k("Digit1"))).toEqual({ kind: "option", index: 0, confidence: "Sure" });
    expect(mockKey(k("Digit3", { shiftKey: true, key: "#" }))).toEqual({
      kind: "option",
      index: 2,
      confidence: "Unsure",
    });
    expect(mockKey(k("Digit9", { altKey: true, key: "ª" }))).toEqual({ kind: "option", index: 8, confidence: "Guess" });
    expect(mockKey(k("Numpad2"))).toEqual({ kind: "option", index: 1, confidence: "Sure" });
  });
  it("has next and flag, and ignores browser shortcuts", () => {
    expect(mockKey(k("Space"))).toEqual({ kind: "next" });
    expect(mockKey(k("KeyF"))).toEqual({ kind: "flag" });
    expect(mockKey(k("Digit1", { metaKey: true }))).toBeNull();
    expect(mockKey(k("Digit0"))).toBeNull();
    expect(mockKey(k("KeyX"))).toBeNull();
  });
});

describe("flashcard keys", () => {
  it("reveals with Space and rates 1–4", () => {
    expect(flashKey(k("Space"))).toEqual({ kind: "reveal" });
    expect(flashKey(k("Digit1"))).toEqual({ kind: "rate", rating: "Again" });
    expect(flashKey(k("Digit4"))).toEqual({ kind: "rate", rating: "Easy" });
    expect(flashKey(k("Digit5"))).toBeNull();
    expect(flashKey(k("Digit2", { ctrlKey: true }))).toBeNull();
  });
});

it("knows when the user is typing", () => {
  const input = document.createElement("input");
  const div = document.createElement("div");
  expect(typingIn(input)).toBe(true);
  const box = document.createElement("input");
  box.type = "checkbox";
  expect(typingIn(box)).toBe(false);
  expect(typingIn(div)).toBe(false);
  expect(typingIn(null)).toBe(false);
});
