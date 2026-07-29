import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/svelte";
import { createRawSnippet } from "svelte";
import Sparkle from "./Sparkle.svelte";

function mockMatchMedia(matches: boolean) {
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches,
    media: query,
    addListener: () => {},
    removeListener: () => {},
    addEventListener: () => {},
    removeEventListener: () => {},
    dispatchEvent: () => false,
  }));
}

function textSnippet(text: string) {
  return createRawSnippet(() => ({
    render: () => `<span>${text}</span>`,
  }));
}

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("Sparkle", () => {
  it("renders its children", () => {
    mockMatchMedia(false);
    const { getByText } = render(Sparkle, { props: { children: textSnippet("hi") } });
    expect(getByText("hi")).toBeTruthy();
  });

  it("does not render the particle layer when reduced motion is preferred", () => {
    mockMatchMedia(true);
    const { container } = render(Sparkle, { props: { children: textSnippet("hi") } });
    expect(container.querySelector(".layer")).toBeNull();
  });

  it("renders the particle layer when reduced motion is not preferred", () => {
    mockMatchMedia(false);
    const { container } = render(Sparkle, { props: { children: textSnippet("hi") } });
    expect(container.querySelector(".layer")).not.toBeNull();
  });
});
