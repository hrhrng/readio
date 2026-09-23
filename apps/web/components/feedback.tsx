import { CheckCircle2, CircleAlert, Info } from "lucide-react"

interface FeedbackProps {
  tone?: "neutral" | "success" | "danger"
  children: React.ReactNode
  className?: string
}

function FeedbackIcon({ tone }: { tone: NonNullable<FeedbackProps["tone"]> }) {
  const Icon =
    tone === "success" ? CheckCircle2 : tone === "danger" ? CircleAlert : Info
  return <Icon className="ui-icon" aria-hidden="true" />
}

export function InlineFeedback({
  tone = "neutral",
  children,
  className,
}: FeedbackProps) {
  return (
    <div
      className={`inline-feedback ${className ?? ""}`.trim()}
      data-tone={tone}
      role={tone === "danger" ? "alert" : "status"}
    >
      <FeedbackIcon tone={tone} />
      <span>{children}</span>
    </div>
  )
}

interface StatusBannerProps extends FeedbackProps {
  action?: React.ReactNode
  dismiss?: React.ReactNode
}

export function StatusBanner({
  tone = "neutral",
  children,
  action,
  dismiss,
  className,
}: StatusBannerProps) {
  return (
    <div
      className={`status-banner ${className ?? ""}`.trim()}
      data-tone={tone}
      role={tone === "danger" ? "alert" : "status"}
    >
      <FeedbackIcon tone={tone} />
      <span className="min-w-0 flex-1">{children}</span>
      {action}
      {dismiss}
    </div>
  )
}
