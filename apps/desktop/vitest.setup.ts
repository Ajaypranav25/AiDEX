import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

// jsdom has no layout, so nothing scrolls; pages still reset their scroll position on navigation.
Element.prototype.scrollTo ??= () => {};

afterEach(cleanup);
