# \RpcApi

All URIs are relative to *https://nockblocks.com*

Method | HTTP request | Description
------------- | ------------- | -------------
[**rpc**](RpcApi.md#rpc) | **POST** /rpc | Nockchain JSON-RPC endpoint
[**rpc_options**](RpcApi.md#rpc_options) | **OPTIONS** /rpc | Handle CORS preflight requests



## rpc

> models::JsonRpcResponseEnvelope rpc(json_rpc_request_envelope)
Nockchain JSON-RPC endpoint

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**json_rpc_request_envelope** | [**JsonRpcRequestEnvelope**](JsonRpcRequestEnvelope.md) |  | [required] |

### Return type

[**models::JsonRpcResponseEnvelope**](JsonRpcResponseEnvelope.md)

### Authorization

No authorization required

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## rpc_options

> String rpc_options()
Handle CORS preflight requests

### Parameters

This endpoint does not need any parameter.

### Return type

**String**

### Authorization

No authorization required

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: text/plain

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)

