import { ChevronRight, ShieldAlert } from "lucide-react";
import {
  Badge,
  Box,
  Button,
  Flex,
  Stack,
  Text,
  Textarea,
} from "@mantine/core";
import { useState } from "react";
import type { ApprovalDecision } from "@/api/types/ApprovalDecision";
import type { ApprovalDto } from "@/api/types/ApprovalDto";
import type { ApprovalItem } from "./types";

const FALLBACK_OPTIONS: Array<{
  id: string;
  kind: ApprovalDecision;
  label: string;
}> = [
  { id: "once", kind: "allow_once", label: "Allow once" },
  { id: "thread", kind: "allow_thread", label: "Allow for thread" },
  { id: "deny", kind: "deny", label: "Deny" },
];

export interface ApprovalRowProps {
  item: ApprovalItem;
  /** Latest server copy, so the row reflects decisions made elsewhere. */
  approval: ApprovalDto;
  deciding?: boolean;
  onDecide?: (
    approvalId: string,
    decision: ApprovalDecision,
    note?: string,
  ) => void;
}

/**
 * Approval request card. Owns the deny-with-reason state so the timeline
 * component stays stateless apart from the presence target.
 */
export function ApprovalRow({
  item,
  approval,
  deciding = false,
  onDecide,
}: ApprovalRowProps) {
  const [denyOpen, setDenyOpen] = useState(false);
  const [denyNote, setDenyNote] = useState("");
  const resolved = item.resolved || approval.status !== "pending";
  const options =
    approval.options.length > 0 ? approval.options : FALLBACK_OPTIONS;

  return (
    <Flex gap="md">
      <Box w={28} />
      <Box
        flex={1}
        miw={0}
        style={{
          border: "1px solid var(--mantine-color-yellow-light)",
          borderRadius: "var(--mantine-radius-md)",
          background: "var(--mantine-color-yellow-light)",
          overflow: "hidden",
        }}
      >
        <Flex align="center" gap="xs" px="sm" py="xs">
          <ShieldAlert
            size={14}
            aria-hidden
            style={{
              color: "var(--mantine-color-yellow-light-color)",
              flexShrink: 0,
            }}
          />
          <Text size="sm" fw={500} truncate="end" flex={1} miw={0}>
            {approval.title}
          </Text>
          <Badge
            variant="light"
            color={resolved ? "gray" : "yellow"}
            size="xs"
            style={{ marginLeft: "auto" }}
          >
            {item.decision ?? approval.status}
          </Badge>
        </Flex>
        <Box
          px="sm"
          py="xs"
          style={{
            borderTop: "1px solid var(--mantine-color-yellow-light-hover)",
          }}
        >
          <Stack gap={4}>
            <Text size="sm" c="dimmed">
              {approval.agent}
              {approval.subagentId ? ` · subagent ${approval.subagentId}` : ""}
            </Text>
            {approval.command ? (
              <Box
                component="pre"
                p="xs"
                style={{
                  margin: 0,
                  overflowX: "auto",
                  fontFamily: "var(--mantine-font-family-monospace)",
                  fontSize: "var(--mantine-font-size-md)",
                  background: "var(--mantine-color-default)",
                  borderRadius: "var(--mantine-radius-sm)",
                }}
              >
                {approval.command}
              </Box>
            ) : null}
            {approval.cwd ? (
              <Text
                size="sm"
                ff="monospace"
                c="dimmed"
                style={{ wordBreak: "break-all" }}
              >
                cwd: {approval.cwd}
              </Text>
            ) : null}
            {approval.paths.length > 0 ? (
              <Text
                size="sm"
                ff="monospace"
                c="dimmed"
                style={{ wordBreak: "break-all" }}
              >
                {approval.paths.join(", ")}
              </Text>
            ) : null}
            {approval.input ? (
              <details style={{ fontSize: "var(--mantine-font-size-md)" }}>
                <summary
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: "0.5rem",
                    cursor: "pointer",
                    listStyle: "none",
                    color: "var(--mantine-color-dimmed)",
                  }}
                >
                  <ChevronRight
                    size={12}
                    data-disclosure-chevron
                    aria-hidden
                    style={{ flexShrink: 0 }}
                  />
                  Full parameters
                </summary>
                <Box
                  component="pre"
                  p="xs"
                  mah={224}
                  mt={4}
                  style={{
                    margin: 0,
                    overflow: "auto",
                    fontFamily: "var(--mantine-font-family-monospace)",
                    fontSize: "var(--mantine-font-size-md)",
                    whiteSpace: "pre-wrap",
                    wordBreak: "break-all",
                    background: "var(--mantine-color-default)",
                    borderRadius: "var(--mantine-radius-sm)",
                  }}
                >
                  {approval.input}
                </Box>
              </details>
            ) : null}
            {!resolved && onDecide ? (
              denyOpen ? (
                <Box
                  pt="xs"
                  style={{
                    borderTop:
                      "1px solid var(--mantine-color-yellow-light-hover)",
                  }}
                >
                  <Textarea
                    autoFocus
                    value={denyNote}
                    onChange={(event) => setDenyNote(event.target.value)}
                    placeholder="Why deny?"
                    rows={2}
                  />
                  <Flex gap="xs" mt="xs">
                    <Button
                      variant="subtle"
                      color="dark"
                      size="sm"
                      className="tb-action-40"
                      disabled={deciding}
                      onClick={() => {
                        setDenyOpen(false);
                        setDenyNote("");
                      }}
                    >
                      Cancel
                    </Button>
                    <Button
                      color="red"
                      size="sm"
                      className="tb-action-40"
                      loading={deciding}
                      disabled={deciding || !denyNote.trim()}
                      onClick={() =>
                        onDecide(approval.id, "deny", denyNote.trim())
                      }
                    >
                      Deny with reason
                    </Button>
                  </Flex>
                </Box>
              ) : (
                <Flex
                  gap="xs"
                  wrap="wrap"
                  pt="xs"
                  style={{
                    borderTop:
                      "1px solid var(--mantine-color-yellow-light-hover)",
                  }}
                >
                  {options.map((option) =>
                    option.kind === "deny" ? (
                      <Button
                        key={option.id}
                        size="sm"
                        className="tb-action-40"
                        variant="subtle"
                        color="red"
                        disabled={deciding}
                        onClick={() => {
                          setDenyOpen(true);
                          setDenyNote("");
                        }}
                      >
                        {option.label}
                      </Button>
                    ) : (
                      <Button
                        key={option.id}
                        size="sm"
                        className="tb-action-40"
                        // Allow-once is the common path; standing rules stay
                        // quieter so a click does not silently widen access.
                        variant={
                          option.kind === "allow_once" ? "filled" : "light"
                        }
                        color={option.kind === "allow_once" ? undefined : "gray"}
                        loading={deciding}
                        disabled={deciding}
                        onClick={() => onDecide(approval.id, option.kind)}
                      >
                        {option.label}
                      </Button>
                    ),
                  )}
                </Flex>
              )
            ) : null}
          </Stack>
        </Box>
      </Box>
    </Flex>
  );
}
