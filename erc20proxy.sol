pragma solidity ^0.8.17;

import "https://github.com/nearprotocol/nearlib/blob/master/contracts/near-api-wrappers/near-api-wrappers.sol";

contract ERC20ConversionProxy {
  // Mapping to store the oracle account
  mapping(bytes32 => address) public oracleAccounts;

  // Mapping to store the provider account
  mapping(bytes32 => address) public providerAccounts;

  // Owner of the contract
  address public owner;

  // Token contract address
  address public tokenAddress;

  // Oracle contract address
  address public oracleAddress;

  // Provider contract address
  address public providerAddress;

  // Struct to represent a payment request
  struct PaymentRequest {
    uint256 amount;
    bytes32 currency;
    address to;
    uint256 feeAmount;
    address feeAddress;
    uint256 maxRateTimespan;
    bytes32 paymentReference;
  }

  // Event to be emitted when a payment is made
  event TransferWithReference(
    uint256 cryptoAmount,
    uint256 cryptoFeeAmount,
    PaymentRequest request
  );

  function transfer_with_reference(
    bytes32 _currency,
    uint256 _amount,
    address _to,
    uint256 _fee_amount,
    address _fee_address,
    uint256 _max_rate_timespan,
    bytes32 _payment_reference
  ) public {
    // Check if the oracle's conversion rate is older than the max_rate_timespan value
    if (_max_rate_timespan != 0 && oracleAddress.getTimestamp() > _max_rate_timespan) {
      // Throw an error if the conversion rate is too old
      revert();
    }

    // Get the current conversion rate from the oracle
    uint256 conversionRate = oracleAddress.getConversionRate(_currency);

    // Calculate the number of tokens to be transferred
    uint256 tokenAmount = _amount * conversionRate;

    // Calculate the number of tokens for the fee
    uint256 feeTokenAmount = _fee_amount * conversionRate;

    // Transfer the tokens to the request issuer
    tokenAddress.transfer(_to, tokenAmount);

    // Transfer the tokens for the fee
    tokenAddress.transfer(_fee_address, feeTokenAmount);

    // Emit the event
    emit TransferWithReference(
      tokenAmount,
      feeTokenAmount,
      PaymentRequest(
        _amount,
        _currency,
        _to,
        _fee_amount,
        _fee_address,
        _max_rate_timespan,
        _payment_reference
      )
    );
  }

  function set_oracle_account(bytes32 _currency, address _oracle) public {
    require(msg.sender == owner, "Only the owner can set the oracle account.");
    oracleAccounts[_currency] = _oracle;
  }

  function get_oracle_account(bytes32 _currency) public view returns (address) {
    return oracleAccounts[_currency];
  }

  function set_provider_account(bytes32 _currency, address _provider) public {
    require(msg.sender == owner, "Only the owner can set the provider account.");
    providerAccounts[_currency] = _provider;
  }

  function get_provider_account(bytes32 _currency) public view returns (address) {
    return providerAccounts[_currency];
  }

function set_owner(address _owner) public {
    require(msg.sender == owner, "Only the owner can set the owner of the contract.");
    owner = _owner;
  }
}

