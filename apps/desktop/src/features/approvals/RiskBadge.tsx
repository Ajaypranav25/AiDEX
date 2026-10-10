import { AlertTriangleIcon, CheckCircleIcon, InfoIcon } from "@aidex/ui/icons";
import { Badge } from "@aidex/ui/primitives";
import type { ApprovalRisk } from "./sample";

const badgeByRisk = {
  high: { tone: "accent", label: "High risk", Icon: AlertTriangleIcon },
  medium: { tone: "neutral", label: "Medium risk", Icon: InfoIcon },
  low: { tone: "success", label: "Low risk", Icon: CheckCircleIcon },
} as const;

/** Risk level as text plus icon, so it never relies on color alone. */
export function RiskBadge({ risk }: { risk: ApprovalRisk }) {
  const { tone, label, Icon } = badgeByRisk[risk];
  return (
    <Badge tone={tone} icon={<Icon size={14} />}>
      {label}
    </Badge>
  );
}
