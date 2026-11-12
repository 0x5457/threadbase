import { Select } from "@mantine/core";
import type { ShareMember } from "../../api";

const roles = [
  { value: "editor", label: "Editor" },
  { value: "viewer", label: "Viewer" },
  { value: "owner", label: "Owner" },
];

export function RoleSelect({
  value,
  size,
  onChange,
}: {
  value: ShareMember["role"];
  size: "xs" | "sm";
  onChange: (role: ShareMember["role"] | null) => void;
}) {
  return (
    <Select
      size={size}
      w={100}
      data={roles}
      value={value}
      allowDeselect={false}
      onChange={(next) => onChange(next as ShareMember["role"] | null)}
    />
  );
}
