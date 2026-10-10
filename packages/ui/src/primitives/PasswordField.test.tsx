import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { PasswordField } from "./PasswordField";

describe("PasswordField", () => {
  it("hides the value and keeps it away from spelling services", () => {
    render(<PasswordField label="Passphrase" defaultValue="correct horse" />);

    const field = screen.getByLabelText("Passphrase");

    expect(field).toHaveAttribute("type", "password");
    expect(field).toHaveAttribute("spellcheck", "false");
    expect(field).toHaveAttribute("autocapitalize", "off");
  });

  it("reveals and hides the value with a toggle that reports its state", async () => {
    const user = userEvent.setup();
    render(<PasswordField label="Passphrase" revealLabel="Show passphrase" defaultValue="correct horse" />);
    const toggle = screen.getByRole("button", { name: "Show passphrase" });

    await user.click(toggle);

    expect(screen.getByLabelText("Passphrase")).toHaveAttribute("type", "text");
    expect(toggle).toHaveAttribute("aria-pressed", "true");

    await user.click(toggle);

    expect(screen.getByLabelText("Passphrase")).toHaveAttribute("type", "password");
    expect(toggle).toHaveAttribute("aria-pressed", "false");
  });
});
