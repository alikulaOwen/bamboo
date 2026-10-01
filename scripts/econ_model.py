#!/usr/bin/env python3
"""
Project Bamboo: Unit Economics & Sustainability Engine
Reproduces per-ticket unit economics, contribution margins, and monthly break-even analysis.
Usage: python3 scripts/econ_model.py
"""

def safaricom_b2c_fee_kes(payout_kes: float) -> float:
    """Official Safaricom M-PESA Bulk Payment Tariff Form (B2C registered users)."""
    if payout_kes <= 100:
        return 0.0
    elif payout_kes <= 1500:
        return 5.0
    elif payout_kes <= 5000:
        return 9.0
    elif payout_kes <= 20000:
        return 11.0
        
    elif payout_kes <= 250000:
        return 13.0
    else:
        raise ValueError(f"Payout KES {payout_kes} exceeds single-transaction cap of KES 250,000")

def compute_order_economics(
    ticket_usdc: float,
    rate_usd_kes: float = 130.50,
    host_fee_bps: int = 100,
    refill_spread_bps: int = 25,
    excise_duty_rate: float = 0.10,
    fraud_provision_bps: int = 5,
    float_turnover_hours: float = 12.0,
    annual_cost_of_capital: float = 0.12,
):
    fee_revenue = ticket_usdc * (host_fee_bps / 10000.0)
    payout_usdc = ticket_usdc - fee_revenue
    payout_kes = payout_usdc * rate_usd_kes
    
    b2c_fee_kes = safaricom_b2c_fee_kes(payout_kes)
    b2c_fee_usdc = b2c_fee_kes / rate_usd_kes
    
    refill_spread_cost = ticket_usdc * (refill_spread_bps / 10000.0)
    excise_duty_cost = fee_revenue * excise_duty_rate
    fraud_provision_cost = ticket_usdc * (fraud_provision_bps / 10000.0)
    
    float_cost_factor = annual_cost_of_capital * (float_turnover_hours / 8760.0)
    float_cost = ticket_usdc * float_cost_factor
    
    total_costs = b2c_fee_usdc + refill_spread_cost + excise_duty_cost + fraud_provision_cost + float_cost
    net_margin_usdc = fee_revenue - total_costs
    net_margin_bps = (net_margin_usdc / ticket_usdc) * 10000.0
    
    return {
        "ticket_usdc": ticket_usdc,
        "payout_kes": payout_kes,
        "fee_revenue": fee_revenue,
        "b2c_fee_kes": b2c_fee_kes,
        "b2c_fee_usdc": b2c_fee_usdc,
        "b2c_fee_bps": (b2c_fee_usdc / ticket_usdc) * 10000.0,
        "refill_spread_cost": refill_spread_cost,
        "excise_duty_cost": excise_duty_cost,
        "fraud_provision_cost": fraud_provision_cost,
        "float_cost": float_cost,
        "total_costs": total_costs,
        "net_margin_usdc": net_margin_usdc,
        "net_margin_bps": net_margin_bps,
    }

def print_summary():
    tickets = [10.0, 25.0, 100.0, 500.0]
    print("=" * 80)
    print("PROJECT BAMBOO: UNIT ECONOMICS TABLE (100 BPS HOST FEE)")
    print("=" * 80)
    print(f"{'Ticket':<8} | {'Payout KES':<12} | {'B2C (KES)':<9} | {'B2C bps':<8} | {'Net (25bps OTC)':<16} | {'Net (50bps OTC)':<16}")
    print("-" * 80)
    for t in tickets:
        e25 = compute_order_economics(t, refill_spread_bps=25)
        e50 = compute_order_economics(t, refill_spread_bps=50)
        print(
            f"${t:<7.2f} | KES {e25['payout_kes']:<8.2f} | KES {e25['b2c_fee_kes']:<5.1f} | {e25['b2c_fee_bps']:<6.1f} bps | "
            f"${e25['net_margin_usdc']:>+6.3f} ({e25['net_margin_bps']:>+5.1f} bps) | "
            f"${e50['net_margin_usdc']:>+6.3f} ({e50['net_margin_bps']:>+5.1f} bps)"
        )
    print("=" * 80)
    
    monthly_fixed_opex = 1250.0
    e100 = compute_order_economics(100.0, refill_spread_bps=35)
    margin_ratio = e100["net_margin_usdc"] / 100.0
    breakeven_monthly_vol = monthly_fixed_opex / margin_ratio
    print(f"\nMonthly Fixed Opex Placeholder: ${monthly_fixed_opex:.2f}")
    print(f"Benchmark Net Margin at $100 ticket (35 bps OTC): {e100['net_margin_bps']:.1f} bps")
    print(f"Monthly Break-Even Volume: ${breakeven_monthly_vol:,.2f} USDC/month (${breakeven_monthly_vol/30.0:,.2f}/day)")
    print("=" * 80)

if __name__ == "__main__":
    print_summary()
