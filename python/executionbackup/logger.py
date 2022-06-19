from typing import *
from asyncio import iscoroutinefunction


listeners: Dict[str, Callable] = {}

def listener(function_name: str):
    """Registers a function as a listener.
    Example usage:
        @router.listener('node_offline')
        async def node_offline(node_url):
            print(f'Node {node_url} is offline')"""
    def decorator(func):
        if not iscoroutinefunction(func):
            raise TypeError('Handler must be a coroutine function')
        if function_name in listeners:
            raise ValueError('Handler already registered')
        listeners[function_name] = func
        return func
    return decorator

async def dispatch(function_name: str, *args, **kwargs):
    if function_name not in listeners:
        return
    return await listeners[function_name](*args, **kwargs)