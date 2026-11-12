import { Group, Text } from "@mantine/core";
import { IconChevronDown } from "@tabler/icons-react";
import type { ToolCallData } from "../../api";
import type { ViewItem } from "../../lib/thread-events";
import { DiffView } from "../DiffView";
import { StatusDot } from "./StatusDot";
import { describeTitle } from "./format";

export function Tool({ item }: { item: Extract<ViewItem, { kind: "tool" }> }) {
  const call: ToolCallData = item.call;
  const path = call.locations[0]?.path;
  return (
    <div className="block">
      <div className="block-gutter" />
      <div className="block-body">
        <Group gap={6} className="tool-head" wrap="nowrap">
          <Text size="sm">
            <span className="verb">{describeTitle(call.title)}</span>
            {path ? (
              <>
                {" "}
                <span className="path">{path}</span>
              </>
            ) : null}
          </Text>
          <StatusDot status={call.status} />
        </Group>
        {call.diff?.patch ? (
          <DiffView patch={call.diff.patch} />
        ) : call.input || call.output ? (
          <details className="tool-details">
            <summary>
              Details <IconChevronDown size={11} stroke={1.5} />
            </summary>
            {call.input ? <pre className="cmd">{call.input}</pre> : null}
            {call.output ? <pre className="cmd">{call.output}</pre> : null}
          </details>
        ) : null}
      </div>
    </div>
  );
}
