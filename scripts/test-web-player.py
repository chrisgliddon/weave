#!/usr/bin/env python3
"""Browser-level smoke test for the generated Weave WASM example."""

from playwright.sync_api import expect, sync_playwright


def main() -> None:
    errors: list[str] = []
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        page = browser.new_page(viewport={"width": 1440, "height": 1000})
        page.on("pageerror", lambda error: errors.append(str(error)))
        page.on(
            "console",
            lambda message: errors.append(message.text) if message.type == "error" else None,
        )
        page.goto("http://127.0.0.1:4173", wait_until="networkidle")

        expect(page.get_by_role("heading", name="Night Observatory")).to_be_visible()
        expect(page.get_by_role("status")).to_contain_text("Line delivered")
        expect(page.locator("#transcript li")).to_have_count(1)
        expect(page.locator(".draw-note")).to_contain_text("Signal drawn")

        opening_draw = page.locator(".draw-note").text_content()
        assert opening_draw is not None
        page.get_by_role("button", name="Continue").click()
        expect(page.get_by_role("group", name="Choose the next action")).to_be_visible()
        expect(page.locator(".choice-button")).to_have_count(2)
        page.get_by_role("button", name="Follow the signal").click()
        expect(page.locator("#transcript")).to_contain_text("road of light")
        page.get_by_role("button", name="Continue").click()
        expect(page.get_by_role("status")).to_have_text("Story complete")

        page.get_by_role("button", name="Restart").click()
        expect(page.locator("#transcript li")).to_have_count(1)
        expect(page.locator(".draw-note")).to_have_text(opening_draw)

        page.get_by_role("button", name="Save state").click()
        expect(page.get_by_role("status")).to_have_text("State saved by this page")
        page.get_by_role("button", name="Continue").click()
        page.get_by_role("button", name="Close the night log").click()
        page.get_by_role("button", name="Load state").click()
        expect(page.get_by_role("status")).to_have_text("Saved runtime state restored")
        expect(page.locator("#transcript li")).to_have_count(0)
        page.get_by_role("button", name="Continue").click()
        expect(page.get_by_role("group", name="Choose the next action")).to_be_visible()

        page.locator("#seed").fill("101")
        page.get_by_role("button", name="Apply").click()
        expect(page.get_by_role("status")).to_have_text("Line delivered")
        expect(page.locator("#seed")).to_have_value("101")

        page.wait_for_timeout(600)
        page.screenshot(path="target/web-player-audit.png", full_page=True)

        mobile = browser.new_page(viewport={"width": 390, "height": 844})
        mobile.goto("http://127.0.0.1:4173", wait_until="networkidle")
        expect(mobile.get_by_role("heading", name="Night Observatory")).to_be_visible()
        has_overflow = mobile.evaluate(
            "document.documentElement.scrollWidth > document.documentElement.clientWidth"
        )
        assert not has_overflow, "mobile layout has horizontal overflow"
        mobile.close()
        browser.close()

    assert not errors, f"browser emitted errors: {errors}"
    print("web player browser smoke passed")


if __name__ == "__main__":
    main()
