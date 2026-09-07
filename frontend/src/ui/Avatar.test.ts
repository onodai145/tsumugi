import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render } from "@testing-library/svelte";
import AvatarTestHost from "./Avatar.test.host.svelte";

afterEach(() => cleanup());

describe("Avatar", () => {
  it("isCat=trueのとき耳要素を描画する", () => {
    const { container } = render(AvatarTestHost, { props: { isCat: true } });
    expect(container.querySelector(".ears")).not.toBeNull();
  });

  it("isCat=falseのとき耳要素を描画しない", () => {
    const { container } = render(AvatarTestHost, { props: { isCat: false } });
    expect(container.querySelector(".ears")).toBeNull();
  });

  it("childrenをそのまま描画する", () => {
    const { getByTestId } = render(AvatarTestHost, { props: { isCat: false } });
    expect(getByTestId("inner-img")).not.toBeNull();
  });
});
