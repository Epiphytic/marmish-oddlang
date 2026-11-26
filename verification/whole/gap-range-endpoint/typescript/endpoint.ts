// A wire-role consumer for the range endpoint. Keep each numeric token until
// its field has been checked. JavaScript's parsed number may already be rounded.
class NumericToken {
    constructor(readonly text: string) {}
}

class InvalidEndpoint extends Error {}

function unsigned64(value: unknown): bigint {
    if (!(value instanceof NumericToken) || !/^(0|[1-9][0-9]{0,19})$/.test(value.text)) {
        throw new InvalidEndpoint();
    }
    const exact = BigInt(value.text);
    if (exact > 18446744073709551615n) throw new InvalidEndpoint();
    return exact;
}

export function roundTrip(wire: string): string | null {
    const record = JSON.parse(wire, (_key: string, value: unknown, context?: { source?: string }) => {
        if (typeof value !== "number") return value;
        if (context?.source === undefined) throw new Error("JSON source text access is required");
        return new NumericToken(context.source);
    });
    if (record === null || Array.isArray(record) || typeof record !== "object"
        || typeof record.reason !== "string") {
        throw new Error("expected an object with a reason string for the endpoint witness");
    }

    try {
        const start = unsigned64(record.start);
        const end: bigint | null = record.end == null ? null : unsigned64(record.end);
        return `{"start":${start},"end":${end},"reason":${JSON.stringify(record.reason)}}`;
    } catch (error) {
        if (error instanceof InvalidEndpoint) return null;
        throw error;
    }
}
