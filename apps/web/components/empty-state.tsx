import { LucideIcon } from "lucide-react";

interface EmptyStateProps {
  icon: LucideIcon;
  title: string;
  description?: string;
}

export function EmptyState({ icon: Icon, title, description }: EmptyStateProps) {
  return (
    <div className="flex flex-col items-center justify-center py-20 text-center">
      <Icon size={48} strokeWidth={1} className="text-text-tertiary mb-4" />
      <h3 className="text-lg font-medium text-text-primary">{title}</h3>
      {description && (
        <p className="text-sm text-text-secondary mt-2 max-w-md">
          {description}
        </p>
      )}
    </div>
  );
}
