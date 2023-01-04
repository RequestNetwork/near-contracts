pragma solidity ^0.6.0;

import "erc20proxy.sol";

contract TestERC20ConversionProxy is ERC20ConversionProxy {
  describe("transfer_with_reference", () => {
    it("should transfer tokens and emit the TransferWithReference event", async () => {
      const contract = await ERC20ConversionProxy.new();
      const amount = "1000000";
      const currency = "USD";
      const tokenAddress = "a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48.factory.bridge.near";
      const feeAddress = "fee.requestfinance.near";
      const feeAmount = "200";
      const maxRateTimespan = "0";
      const paymentReference = "abc7c8bb1234fd12";
      const to = "dummy.payee.near";

      // Transfer tokens and emit the TransferWithReference event
      const result = await contract.transfer_with_reference(
        amount,
        currency,
        tokenAddress,
        feeAddress,
        feeAmount,
        maxRateTimespan,
        paymentReference,
        to,
        { value: amount }
      );

      // Check that the TransferWithReference event was emitted
      assert.equal(result.logs[0].event, "TransferWithReference");

      // Check that the event contains the correct information
      assert.equal(result.logs[0].args.amount, amount);
      assert.equal(result.logs[0].args.currency, currency);
      assert.equal(result.logs[0].args.tokenAddress, tokenAddress);
      assert.equal(result.logs[0].args.feeAddress, feeAddress);
      assert.equal(result.logs[0].args.feeAmount, feeAmount);
      assert.equal(result.logs[0].args.maxRateTimespan, maxRateTimespan);
      assert.equal(result.logs[0].args.paymentReference, paymentReference);
      assert.equal(result.logs[0].args.to, to);
    });

    it("should throw an error if the conversion rate is too old", async () => {
      // Set the max rate timespan to a non-zero value
      const maxRateTimespan = 3600;
      await contract.setMaxRateTimespan(maxRateTimespan);

      // Set the conversion rate to be older than the max rate timespan
      const conversionRate = 100;
      const conversionRateTimestamp = Math.floor(Date.now() / 1000) - maxRateTimespan - 1;
      await contract.setConversionRate(conversionRate, conversionRateTimestamp);
      // Try to call the transfer_with_reference function
      const amount = 100;
      const currency = "USD";
      const tokenAddress = "usdc.e";
      const feeAddress = "fee.requestfinance.near";
      const feeAmount = 2;
      const paymentReference = "abc7c8bb1234fd12";
      const to = "dummy.payee.near";
      await assert.throws(
        contract.transfer_with_reference(amount, currency, tokenAddress, feeAddress, feeAmount, paymentReference, to),
        "The payer does not have enough funds"
      );
    });
  });
}