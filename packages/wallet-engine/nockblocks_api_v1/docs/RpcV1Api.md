# \RpcV1Api

All URIs are relative to *https://nockblocks.com*

Method | HTTP request | Description
------------- | ------------- | -------------
[**rpc_v1**](RpcV1Api.md#rpc_v1) | **POST** /rpc/v1 | Nockchain JSON-RPC v1 endpoint
[**rpc_v1_options**](RpcV1Api.md#rpc_v1_options) | **OPTIONS** /rpc/v1 | Handle CORS preflight requests



## rpc_v1

> models::JsonRpcResponseEnvelope rpc_v1(json_rpc_request_envelope)
Nockchain JSON-RPC v1 endpoint

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


## rpc_v1_options

> rpc_v1_options()
Handle CORS preflight requests

### Parameters

This endpoint does not need any parameter.

### Return type

 (empty response body)

### Authorization

No authorization required

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: Not defined

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)

