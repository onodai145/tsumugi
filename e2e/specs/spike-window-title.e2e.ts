describe("tauri-driver spike", () => {
  it("gets the window title", async () => {
    const title = await browser.getTitle();
    expect(title).toBeDefined();
    expect(title.length).toBeGreaterThan(0);
  });
});
