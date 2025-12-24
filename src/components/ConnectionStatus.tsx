import { useState, useEffect, useCallback, useRef } from "react";
import { api } from "@/lib/tauri";
import { Spinner } from "@/components/ui/spinner";
import { Button } from "@/components/ui/button";
import {
    Tooltip,
    TooltipContent,
    TooltipTrigger,
} from "@/components/ui/tooltip";
import { ArrowsClockwise } from "@phosphor-icons/react";

type Status = "connected" | "disconnected" | "connecting" | "reconnecting";

interface ConnectionStatusProps {
    connectionUuid: string;
    onStatusChange?: (status: Status) => void;
}

export function ConnectionStatus({
    connectionUuid,
    onStatusChange,
}: ConnectionStatusProps) {
    const [status, setStatus] = useState<Status>("connecting");
    const [error, setError] = useState<string | null>(null);
    const [isConnecting, setIsConnecting] = useState(false);
    const hasConnectedOnce = useRef(false);
    const isMounted = useRef(true);

    const connect = useCallback(async (isInitial: boolean = false) => {
        if (!isMounted.current) return;

        setIsConnecting(true);
        setStatus(isInitial ? "connecting" : "reconnecting");

        try {
            const result = await api.pool.connect(connectionUuid);
            if (!isMounted.current) return;

            setStatus(result.status as Status);
            setError(result.error || null);
            onStatusChange?.(result.status as Status);

            if (result.status === "connected") {
                hasConnectedOnce.current = true;
            }
        } catch (err) {
            if (!isMounted.current) return;
            setStatus("disconnected");
            setError(err instanceof Error ? err.message : "Connection failed");
        } finally {
            if (isMounted.current) {
                setIsConnecting(false);
            }
        }
    }, [connectionUuid, onStatusChange]);

    // Initial connection on mount
    useEffect(() => {
        isMounted.current = true;
        hasConnectedOnce.current = false;
        connect(true);

        return () => {
            isMounted.current = false;
        };
    }, [connect]);

    // Periodic health check
    useEffect(() => {
        const interval = setInterval(() => {
            if (status === "connected" && isMounted.current) {
                api.pool.healthCheck(connectionUuid).catch(() => {
                    if (isMounted.current) {
                        setStatus("disconnected");
                    }
                });
            }
        }, 30000);

        return () => clearInterval(interval);
    }, [connectionUuid, status]);

    const handleReconnect = useCallback(() => {
        connect(false);
    }, [connect]);

    const statusColors = {
        connected: "bg-green-500",
        disconnected: "bg-red-500",
        connecting: "bg-yellow-500",
        reconnecting: "bg-yellow-500",
    };

    const statusLabels = {
        connected: "Connected",
        disconnected: "Disconnected",
        connecting: "Connecting...",
        reconnecting: "Reconnecting...",
    };

    const isLoading = status === "connecting" || status === "reconnecting";

    return (
        <div className="flex items-center gap-2">
            <Tooltip>
                <TooltipTrigger
                    render={
                        <div className="flex items-center gap-1.5 cursor-default">
                            {isLoading ? (
                                <Spinner className="w-3 h-3" />
                            ) : (
                                <span
                                    className={`w-2 h-2 rounded-full ${statusColors[status]}`}
                                />
                            )}
                            <span className="text-xs text-muted-foreground">
                                {statusLabels[status]}
                            </span>
                        </div>
                    }
                />
                <TooltipContent>
                    <p>
                        Status: {statusLabels[status]}
                        {error && <span className="block text-red-400">{error}</span>}
                    </p>
                </TooltipContent>
            </Tooltip>

            {status === "disconnected" && (
                <Button
                    variant="ghost"
                    size="icon-sm"
                    onClick={handleReconnect}
                    disabled={isConnecting}
                    title="Reconnect"
                >
                    <ArrowsClockwise className={`w-3.5 h-3.5 ${isConnecting ? "animate-spin" : ""}`} />
                </Button>
            )}
        </div>
    );
}
