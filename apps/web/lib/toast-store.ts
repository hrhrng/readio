export interface ToastNotice {
  id: string
  message: string
  tone: "neutral" | "success" | "danger"
  politeness: "polite" | "assertive"
}

export type ToastAction =
  | { type: "push"; toast: ToastNotice }
  | { type: "dismiss"; id: string }

const MAX_VISIBLE_TOASTS = 3

export function toastReducer(
  state: ToastNotice[],
  action: ToastAction
): ToastNotice[] {
  if (action.type === "dismiss") {
    return state.filter((toast) => toast.id !== action.id)
  }

  return [...state, action.toast].slice(-MAX_VISIBLE_TOASTS)
}
