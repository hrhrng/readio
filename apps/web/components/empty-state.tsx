import { LucideIcon } from "lucide-react";

interface EmptyStateProps {
  icon: LucideIcon;
  title: string;
  description?: string;
  action?: React.ReactNode;
}

export function EmptyState({
  icon: Icon,
  title,
  description,
  action,
}: EmptyStateProps) {
  return (
    <div className="ui-empty-state">
      <span className="ui-state-icon" aria-hidden="true">
        <Icon className="ui-icon-lg" />
      </span>
      <h2 className="ui-state-title">{title}</h2>
      {description && (
        <p className="ui-state-description">{description}</p>
      )}
      {action}
    </div>
  );
}
