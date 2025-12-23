import { useState, useEffect, useCallback } from "react";
import { api } from "@/lib/tauri";
import { Spinner } from "@/components/ui/spinner";
import { Button } from "@/components/ui/button";
import {
    Tooltip,
    TooltipContent,
    TooltipTrigger,
} from "@/components/ui/tooltip";
import { ArrowsClockwise } from "@phosphor-icons/react";

type Status = "connected" | "disconnected" | "reconnecting";

interface ConnectionStatusProps {
    connectionUuid: string;
    onStatusChange?: (status: Status) => void;
}

export function ConnectionStatus({
    connectionUuid,
    onStatusChange,
}: ConnectionStatusProps) {
    const [status, setStatus] = useState<Status>("disconnected");
    const [error, setError] = useState<string | null>(null);
    const [isConnecting, setIsConnecting] = useState(false);

    const connect = useCallback(async () => {
        setIsConnecting(true);
        setStatus("reconnecting");
        try {
            const result = await api.pool.connect(connectionUuid);
            setStatus(result.status as Status);
            setError(result.error || null);
            onStatusChange?.(result.status as Status);
        } catch (err) {
            setStatus("disconnected");
            setError(err instanceof Error ? err.message : "Connection failed");
        } finally {
            setIsConnecting(false);
        }
    }, [connectionUuid, onStatusChange]);

    // Initial connection on mount
    useEffect(() => {
        connect();
    }, [connect]);

    // Periodic health check
    useEffect(() => {
        const interval = setInterval(() => {
            if (status === "connected") {
                api.pool.healthCheck(connectionUuid).catch(() => {
                    setStatus("disconnected");
                });
            }
        }, 30000); // Check every 30 seconds

        return () => clearInterval(interval);
    }, [connectionUuid, status]);

    const statusColors = {
        connected: "bg-green-500",
        disconnected: "bg-red-500",
        reconnecting: "bg-yellow-500",
    };

    const statusLabels = {
        connected: "Connected",
        disconnected: "Disconnected",
        reconnecting: "Reconnecting...",
    };

    return (
        <div className="flex items-center gap-2">
            <Tooltip>
                <TooltipTrigger
                    render={
                        <div className="flex items-center gap-1.5 cursor-default">
                            {status === "reconnecting" ? (
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
                    onClick={connect}
                    disabled={isConnecting}
                    title="Reconnect"
                >
                    <ArrowsClockwise className={`w-3.5 h-3.5 ${isConnecting ? "animate-spin" : ""}`} />
                </Button>
            )}
        </div>
    );
}
