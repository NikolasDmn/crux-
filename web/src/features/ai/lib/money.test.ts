import { capInput, capText, currencyFor, money } from "./money";

describe("AI spending in your currency", () => {
  it("picks the region's currency", () => {
    expect(currencyFor("nl-NL")).toBe("EUR");
    expect(currencyFor("en-GB")).toBe("GBP");
    expect(currencyFor("en")).toBe("USD");
    expect(currencyFor(undefined)).toBe("USD");
  });
  it("shows USD exactly and others as approximate", () => {
    expect(money(5_000_000, "USD", "en-US")).toBe("$5.00");
    expect(money(5_400_000, "EUR", "en-US")).toBe("≈ €5.00");
    expect(money(0, "EUR", "en-US")).toBe("€0.00");
    expect(money(5_000_000, "JPY", "en-US")).toMatch(/^≈ ¥746$/);
  });
  it("reads a typed cap into micro-USD and back", () => {
    expect(capInput("5", "USD")).toBe(5_000_000);
    expect(capInput("5,00", "EUR")).toBe(5_400_000);
    expect(capInput("€ 3", "EUR")).toBe(3_240_000);
    expect(capInput("", "EUR")).toBe("none");
    expect(capInput("abc", "EUR")).toBeNull();
    expect(capText(5_400_000, "EUR")).toBe("5");
    expect(capText(null, "EUR")).toBe("");
  });
});
