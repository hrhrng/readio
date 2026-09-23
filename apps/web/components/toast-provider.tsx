"use client"

import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useReducer,
  useRef,
} from "react"
import { CheckCircle2, CircleAlert, Info, X } from "lucide-react"
import { resolveFeedback, type FeedbackEvent } from "@/lib/ui-contracts"
import { toastReducer } from "@/lib/toast-store"
import { useLocale } from "./locale-provider"

interface ToastContextValue {
  showToast: (event: FeedbackEvent, message: string) => void
}

const ToastContext = createContext<ToastContextValue | null>(null)

const TOAST_DURATION_MS = 5000

export function ToastProvider({ children }: { children: React.ReactNode }) {
  const { t } = useLocale()
  const [toasts, dispatch] = useReducer(toastReducer, [])
  const nextId = useRef(0)

  const showToast = useCallback((event: FeedbackEvent, message: string) => {
    const contract = resolveFeedback(event)
    const id = String(nextId.current++)

    dispatch({
      type: "push",
      toast: {
        id,
        message,
        tone: contract.tone,
        politeness: contract.politeness,
      },
    })

    window.setTimeout(() => {
      dispatch({ type: "dismiss", id })
    }, TOAST_DURATION_MS)
  }, [])

  const value = useMemo(() => ({ showToast }), [showToast])

  return (
    <ToastContext.Provider value={value}>
      {children}
      <div className="toast-region" aria-label={t("common.notifications")}>
        {toasts.map((toast) => {
          const Icon =
            toast.tone === "success"
              ? CheckCircle2
              : toast.tone === "danger"
                ? CircleAlert
                : Info

          return (
            <div
              key={toast.id}
              className="toast"
              data-tone={toast.tone}
              role={toast.politeness === "assertive" ? "alert" : "status"}
            >
              <Icon className="ui-icon toast-icon" aria-hidden="true" />
              <p className="toast-message">{toast.message}</p>
              <button
                type="button"
                className="ui-icon-button"
                aria-label={t("common.dismissNotification")}
                onClick={() => dispatch({ type: "dismiss", id: toast.id })}
              >
                <X className="ui-icon-sm" aria-hidden="true" />
              </button>
            </div>
          )
        })}
      </div>
    </ToastContext.Provider>
  )
}

export function useToast() {
  const context = useContext(ToastContext)
  if (!context) {
    throw new Error("useToast must be used within ToastProvider")
  }
  return context
}
