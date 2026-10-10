import { SearchIcon } from "../icons/actions";
import { TextField, type TextFieldProps } from "./TextField";

export type SearchFieldProps = Omit<TextFieldProps, "type" | "shape" | "leadingIcon">;

/** Pill-shaped search input with a leading search icon. */
export function SearchField(props: SearchFieldProps) {
  return <TextField type="search" shape="pill" leadingIcon={<SearchIcon />} {...props} />;
}
