import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render } from "@testing-library/svelte";
import CustomEmoji from "./CustomEmoji.svelte";

afterEach(() => cleanup());

describe("CustomEmoji", () => {
  it("renders an img with alt/title when a url is provided", () => {
    const { getByRole } = render(CustomEmoji, {
      props: { name: "blob_cat", url: "https://example.com/e.png" },
    });
    const img = getByRole("img");
    expect(img.getAttribute("src")).toBe("https://example.com/e.png");
    expect(img.getAttribute("alt")).toBe(":blob_cat:");
    expect(img.getAttribute("title")).toBe(":blob_cat:");
  });

  it("omits the title attribute when showTitle is false", () => {
    const { getByRole } = render(CustomEmoji, {
      props: { name: "blob_cat", url: "https://example.com/e.png", showTitle: false },
    });
    expect(getByRole("img").hasAttribute("title")).toBe(false);
  });

  it("renders fallback text when no url is provided", () => {
    const { getByText, queryByRole } = render(CustomEmoji, { props: { name: "blob_cat" } });
    expect(getByText(":blob_cat:")).toBeTruthy();
    expect(queryByRole("img")).toBeNull();
  });
});
