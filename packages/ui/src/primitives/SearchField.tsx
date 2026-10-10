import { SearchIcon } from "../icons/actions";
import { TextField, type TextFieldProps } from "./TextField";

export type SearchFieldProps = Omit<TextFieldProps, "type" | "shape" | "leadingIcon" | "labelVisibility">;

/** Pill-shaped search input with a leading search icon; the label is visually hidden. */
export function SearchField(props: SearchFieldProps) {
  return <TextField type="search" shape="pill" labelVisibility="hidden" leadingIcon={<SearchIcon />} {...props} />;
}
