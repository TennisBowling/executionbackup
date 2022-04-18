
__all__ = ['Account']

class Account:
    def __init__(self, key: int, callAmount=0, callDict: dict={}):
        self.key = key
        self.callAmount = callAmount
        self.callDict = callDict

    
