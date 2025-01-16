using System;
using System.IO;
using System.Text;
using System.Collections.Generic;
using YamlDotNet.Serialization;
using YamlDotNet.Serialization.NamingConventions;
using BfevLibrary;
using Newtonsoft.Json;
//dotnet new console -n AinbBinaryToTextApp
//dotnet publish -c Release -r win-x64 --self-contained true -o publish
//dotnet add package Newtonsoft.Json
//dotnet add package BfevLibrary
//dotnet add package YamlDotNet
class Program
{
    static void Main(string[] args)
    {
        if (args.Length == 0)
        {
            Console.WriteLine("Error: No command provided. Use 'to-json' or 'to-binary'.");
            return;
        }

        switch (args[0])
        {
            case "EvflBinaryToText":
                EvflBinaryToText();
                break;

            case "EvflTextToBinary":
                EvflTextToBinary();
                break;

            default:
                Console.WriteLine("Error: Unknown command. Use 'to-json' or 'to-binary'.");
                break;
        }
    }

    static void EvflTextToBinary()
    {
        try
        {
            // Read binary data from standard input
            using (var input = Console.OpenStandardInput())
            using (var memoryStream = new MemoryStream())
            {
                input.CopyTo(memoryStream);
                byte[] data = memoryStream.ToArray();
                string dataAsString = Encoding.UTF8.GetString(data);

                BfevFile bfev = BfevFile.FromJson(dataAsString);
                byte[] newData = bfev.ToBinary();
                // string jsonString = bfev.ToJson(format: true);
                // string yamlString = ConvertJsonToYaml(jsonString);
                // Console.OutputEncoding = Encoding.UTF8;
                using (var output = Console.OpenStandardOutput())
                {
                    output.Write(newData, 0, newData.Length);
                }

            }
        }
        catch (Exception e)
        {
            // Write error message to standard output
            Console.OutputEncoding = Encoding.UTF8;
            Console.Write($"Error: {e.Message}");
        }
    }

    static void EvflBinaryToText()
    {
        try
        {
            // Read binary data from standard input
            using (var input = Console.OpenStandardInput())
            using (var memoryStream = new MemoryStream())
            {
                input.CopyTo(memoryStream);
                byte[] data = memoryStream.ToArray();

                BfevFile bfev = BfevFile.FromBinary(data);
                string jsonString = bfev.ToJson(format: true);
                // string yamlString = ConvertJsonToYaml(jsonString);
                Console.OutputEncoding = Encoding.UTF8;
                Console.Write(jsonString);

            }
        }
        catch (Exception e)
        {
            // Write error message to standard output
            Console.OutputEncoding = Encoding.UTF8;
            Console.Write($"Error: {e.Message}");
        }
    }

    static string ConvertJsonToYaml(string jsonString)
    {
        // Deserialize JSON to a .NET object (Dictionary or dynamic)
        var jsonObject = JsonConvert.DeserializeObject<Dictionary<string, object>>(jsonString);

        // Serialize the object to YAML
        var serializer = new SerializerBuilder()
            .WithNamingConvention(CamelCaseNamingConvention.Instance)
            .ConfigureDefaultValuesHandling(DefaultValuesHandling.OmitNull)
            .Build();

        string yamlString = serializer.Serialize(jsonObject);
        return yamlString;
    }
}
